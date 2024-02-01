#![allow(dead_code)]
mod types;

use std::{collections::HashMap, path::Path};

use serde::Deserialize;
use snafu::{prelude::*, ResultExt};
use szr_morph::{Blob, Cache, Dict, FormatToken, UserDict};
use szr_tokenise::{AnnToken, AnnTokens, Tokeniser};
use tracing::{error, info, instrument, trace};
use uuid::Uuid;

pub use crate::types::{
    FourthPos, MainPos, SecondPos, Term, TermExtract, ThirdPos, UnidicLemmaId, UnidicSurfaceFormId,
    Unknown,
};

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Error)))]
pub enum Error {
    #[snafu(whatever, display("{message}: {source:?}"))]
    CatchallError {
        message: String,
        #[snafu(source(from(Box<dyn std::error::Error>, Some)))]
        source: Option<Box<dyn std::error::Error>>,
    },
}

fn open_blob(s: &str) -> Result<Blob> {
    Blob::open(&format!("data/system/unidic-cwj-3.1.0/{}", s))
        .whatever_context(format!("loading blob file {s}"))
}

fn load_mecab_dict(user_dict: Vec<(String, String, FormatToken)>) -> Result<Dict> {
    let sysdic = open_blob("sys.dic")?;
    let unkdic = open_blob("unk.dic")?;
    let matrix = open_blob("matrix.bin")?;
    let charbin = open_blob("char.bin")?;
    let mut dict = Dict::load(sysdic, unkdic, matrix, charbin).whatever_context("loading dict")?;
    dict.load_user_dictionary(user_dict)
        .whatever_context("loading userdict")?;
    Ok(dict)
}

pub struct UnidicSession {
    dict: Dict,
    cache: Cache,
}

pub struct AnalysisResult<'a> {
    pub tokens: Vec<(&'a str, UnidicLemmaId)>,
    pub terms: HashMap<UnidicLemmaId, Term>,
}

// TODO: emphatic glottal stops 完ッ全

const NAME_COST: i64 = 5000;

const SEI_LEFT: u16 = 2793;
const SEI_RIGHT: u16 = 11570;
const SEI_POS: &'static str = "名詞,普通名詞,人名,姓";

const MYOU_LEFT: u16 = 357;
const MYOU_RIGHT: u16 = 14993;
const MYOU_POS: &'static str = "名詞,普通名詞,人名,名";

const NOUN_LEFT: u16 = 6812;
const NOUN_RIGHT: u16 = 546;
const NOUN_POS: &'static str = "名詞,普通名詞,一般,*";

#[derive(Deserialize, Debug)]
pub enum NameType {
    Myou,
    Sei,
    Noun,
}

// TODO source text NOT NULL
pub enum LemmaSource {
    Unidic,
    Custom,
}

impl UnidicSession {
    pub fn new(user_dict_path: impl AsRef<Path>) -> Result<Self> {
        let user_dict_raw = Self::build_from_names(user_dict_path)?;
        let user_dict = user_dict_raw
            .into_iter()
            .map(|(l, r, c, i, s, f)| UserDict::build_entry(l, r, c, i, &s, &f))
            .collect();
        let dict = load_mecab_dict(user_dict).whatever_context("loading unidic")?;
        let cache = Cache::new();
        info!("initialised unidic session");
        Ok(Self { dict, cache })
    }

    fn build_unidic_feature_string(
        id: u32,
        pos_str: &str,
        surface: &str,
        kata_rdg: &str,
    ) -> String {
        format!("{pos_str},*,*,{kata_rdg},{surface},{surface},{kata_rdg},{surface},{kata_rdg},漢,*,*,*,*,*,*,体,{kata_rdg},{kata_rdg},{kata_rdg},{kata_rdg},*,*,*,{id},{id}")
    }

    fn build_from_names(
        user_dict_path: impl AsRef<Path>,
    ) -> Result<Vec<(u16, u16, i64, u32, String, String)>> {
        let mut r = Vec::new();
        let mut reader = csv::ReaderBuilder::new().has_headers(false).from_reader(
            std::fs::File::open(user_dict_path.as_ref()).expect("cannot open user dict"),
        );

        let mut i = 0; // self.features.len() as u32;
        for rec in reader.deserialize::<(NameType, String, String)>() {
            let (name_type, surface, kata_rdg) = rec.unwrap();
            let (pos, left, right) = match name_type {
                NameType::Myou => (MYOU_POS, MYOU_LEFT, MYOU_RIGHT),
                NameType::Sei => (SEI_POS, SEI_LEFT, SEI_RIGHT),
                NameType::Noun => (NOUN_POS, NOUN_LEFT, NOUN_RIGHT),
            };

            let feature = Self::build_unidic_feature_string(400_000 + i, pos, &surface, &kata_rdg);
            let entry = (left, right, NAME_COST, i, surface, feature);
            r.push(entry);
            i += 1;
        }
        Ok(r)
    }

    #[instrument(
        skip_all,
        level = "debug",
        fields(main_dict_term_count, user_dict_term_count)
    )]
    pub fn with_terms<T, F: FnMut(LemmaSource, Term) -> Result<()>>(
        main_dict_path: T,
        user_dict_path: Option<T>,
        mut f: F,
    ) -> Result<()>
    where
        T: AsRef<Path>,
    {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_path(main_dict_path.as_ref())
            .whatever_context("csv")?;

        let mut main_dict_term_count = 0;
        for rec_full in rdr.records() {
            // the raw unidic csv contains four extra fields at the beginning
            // ideally i would be able to do serde(flatten) on a local type but
            // it's very finicky with csv apparently
            let rec_full = rec_full.unwrap();
            let mut rec = csv::StringRecord::new();
            for f in rec_full.iter().skip(4) {
                rec.push_field(f);
            }
            let line = rec
                .deserialize::<Term>(None)
                .whatever_context("failed to deserialise record")?;
            f(LemmaSource::Unidic, line)?;
            main_dict_term_count += 1;
        }
        tracing::Span::current().record("main_dict_term_count", main_dict_term_count);

        if let Some(user_dict_path) = user_dict_path {
            let mut user_dict_term_count = 0;
            for user_rec in Self::build_from_names(user_dict_path)? {
                let mut rdr = csv::ReaderBuilder::new()
                    .has_headers(false)
                    .from_reader(user_rec.5.as_bytes());
                let term = rdr.deserialize::<Term>().next().unwrap().unwrap();
                f(LemmaSource::Custom, term)?;
                user_dict_term_count += 1;
            }
            tracing::Span::current().record("user_dict_term_count", user_dict_term_count);
        }

        Ok(())
    }

    pub fn all_terms<T>(path: T, user_dict: Option<T>) -> Result<Vec<Term>>
    where
        T: AsRef<Path>,
    {
        let mut v = Vec::new();
        Self::with_terms(path, user_dict, |_, term| {
            v.push(term);
            Ok(())
        })?;
        Ok(v)
    }

    fn de_to_record<R: std::io::Read>(r: R) -> Result<csv::StringRecord> {
        let r = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(false)
            .from_reader(r)
            .records()
            .next()
            .whatever_context("no feature in stream")?
            .whatever_context("deserialising feature to record")?;
        Ok(r)
    }

    fn analyse_with_cache<'a>(&mut self, input: &'a str) -> Result<AnalysisResult<'a>> {
        Self::analyse_impl(&self.dict, &mut self.cache, input).whatever_context("analyse")
    }

    fn analyse_without_cache<'a>(&self, input: &'a str) -> Result<AnalysisResult<'a>> {
        let mut cache = Cache::new();
        Self::analyse_impl(&self.dict, &mut cache, input).whatever_context("analyse")
    }

    fn analyse_impl<'a>(
        dict: &Dict,
        cache: &mut Cache,
        input: &'a str,
    ) -> Result<AnalysisResult<'a>> {
        let mut tokens = Vec::new();
        let mut terms = HashMap::new();

        let mut buf = Vec::new();

        let cost = dict
            .analyse_with_cache(cache, input, &mut buf)
            .whatever_context("dict")?;
        let cost_per_token = cost as f32 / buf.len() as f32;
        trace!(cost, cost_per_token, "finished tokenising");

        let mut unk_count = 0;

        for token in &buf {
            let text = token.get_text(&input);
            let features_raw = token
                .get_feature(dict)
                .whatever_context("empty feature string")?;
            let rec = Self::de_to_record(features_raw.as_bytes())?;
            if let Ok(term) = rec.deserialize::<Term>(None) {
                let id = term.lemma_id;
                terms.insert(id, term);
                tokens.push((text, id));
            } else if let Ok(_unk) = rec.deserialize::<Unknown>(None) {
                unk_count += 1;
                // FIXME add a real fallback
                tokens.push((text, UnidicLemmaId(0)));
            } else {
                error!("failed to parse csv: {}", features_raw);
            }
        }

        trace!(
            "finished dumping {} tokens ({} unks, {} unique terms)",
            tokens.len(),
            unk_count,
            terms.len()
        );

        Ok(AnalysisResult { tokens, terms })
    }
}

impl Tokeniser for UnidicSession {
    type Error = Error;

    #[instrument(skip_all, level = "trace")]
    fn tokenise<'a>(&self, input: &'a str) -> Result<AnnTokens, Self::Error> {
        let analysis_result = self
            .analyse_without_cache(input)
            .whatever_context("analysis failed")?;
        let mut ret = Vec::new();
        for (token_slice, lemma_id) in analysis_result.tokens {
            let term = &analysis_result.terms.get(&lemma_id);
            let id = term.map(|term| Uuid::from_u64_pair(0, term.lemma_guid.0 as u64));
            ret.push(AnnToken {
                token: token_slice.to_owned(),
                surface_form_id: id,
            })
        }
        Ok(AnnTokens(ret))
    }
}

// Check that we can parse everything that's actually in Unidic.
#[test]
fn unidic_csv_parse() -> Result<()> {
    let unidic_path = "/home/s/c/szr/data/system/unidic-cwj-3.1.0/lex_3_1.csv";
    UnidicSession::with_terms(unidic_path, None, |_, _| Ok(()))
}

// Check that the weirdness of the CSV-parsing adjustments doesn't
// break our ability to roundtrip to json and back.
#[test]
fn unidic_csv_roundtrip_json() -> Result<()> {
    let unidic_path = "/home/s/c/szr/data/system/unidic-cwj-3.1.0/lex_3_1.csv";
    UnidicSession::with_terms(unidic_path, None, |_, term| {
        let json = serde_json::to_string(&term).whatever_context("failed to convert to json")?;
        let roundtrip: Term = serde_json::from_str(&json).whatever_context("roundtrip")?;
        if term != roundtrip {
            println!("      csv: {term:?}");
            println!("     json: {json}");
            println!("roundtrip: {roundtrip:?}");
            todo!("did not match")
        } else {
            Ok(())
        }
    })
}
