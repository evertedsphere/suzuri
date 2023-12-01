#![allow(dead_code)]
use snafu::prelude::*;
use snafu::{ResultExt, Whatever};
use std::collections::HashMap;
use szr_tokenise::{AnnToken, AnnTokens, Tokeniser};
use tracing::{debug, error, info, instrument, warn};

mod types;

use szr_morph::{Blob, Cache, Dict};
pub use types::*;

pub use self::types::LemmaId;

fn open_blob(s: &str) -> Result<crate::Blob, Whatever> {
    Blob::open(&format!("data/system/unidic-cwj-3.1.0/{}", s))
        .whatever_context(format!("loading blob file {s}"))
}

fn load_mecab_dict() -> Result<crate::Dict, Whatever> {
    let sysdic = open_blob("sys.dic")?;
    let unkdic = open_blob("unk.dic")?;
    let matrix = open_blob("matrix.bin")?;
    let charbin = open_blob("char.bin")?;
    let mut dict = Dict::load(sysdic, unkdic, matrix, charbin).whatever_context("loading dict")?;
    dict.load_user_dictionary()
        .whatever_context("loading userdict")?;
    Ok(dict)
}

pub struct UnidicSession {
    dict: Dict,
    cache: Cache,
}

pub struct AnalysisResult<'a> {
    pub tokens: Vec<(&'a str, LemmaId)>,
    pub terms: HashMap<LemmaId, Term>,
}

// TODO: emphatic glottal stops 完ッ全

impl UnidicSession {
    pub fn new() -> Result<Self, Whatever> {
        let dict = load_mecab_dict().whatever_context("loading unidic")?;
        let cache = Cache::new();
        info!("initialised unidic session");
        Ok(Self { dict, cache })
    }

    fn with_terms<F: Fn(Term) -> Result<(), Whatever>>(f: F) -> Result<(), Whatever> {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_path("../data/system/unidic-cwj-3.1.0/lex_3_1.csv")
            .whatever_context("csv")?;

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
            f(line)?;
        }

        Ok(())
    }

    fn de_to_record<R: std::io::Read>(r: R) -> Result<csv::StringRecord, Whatever> {
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

    #[instrument(skip_all)]
    fn analyse_with_cache<'a>(&mut self, input: &'a str) -> Result<AnalysisResult<'a>, Whatever> {
        Self::analyse_impl(&self.dict, &mut self.cache, input)
    }

    #[instrument(skip_all)]
    fn analyse_without_cache<'a>(&self, input: &'a str) -> Result<AnalysisResult<'a>, Whatever> {
        let mut cache = Cache::new();
        Self::analyse_impl(&self.dict, &mut cache, input)
    }

    fn analyse_impl<'a>(
        dict: &Dict,
        cache: &mut Cache,
        input: &'a str,
    ) -> Result<AnalysisResult<'a>, Whatever> {
        let mut tokens = Vec::new();
        let mut terms = HashMap::new();

        let mut buf = Vec::new();

        let cost = dict
            .analyse_with_cache(cache, input, &mut buf)
            .whatever_context("dict")?;
        let cost_per_token = cost as f32 / buf.len() as f32;
        debug!(cost, cost_per_token, "finished tokenising");

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
                tokens.push((text, LemmaId(0)));
            } else {
                error!("failed to parse csv: {}", features_raw);
            }
        }

        debug!(
            "finished dumping {} tokens ({} unks, {} unique terms)",
            tokens.len(),
            unk_count,
            terms.len()
        );

        Ok(AnalysisResult { tokens, terms })
    }
}

impl Tokeniser for UnidicSession {
    type Error = Whatever;

    fn tokenise_mut<'a>(&mut self, input: &'a str) -> Result<AnnTokens<'a>, Self::Error> {
        let analysis_result = self
            .analyse_with_cache(input)
            .whatever_context("analysis failed")?;
        let mut ret = Vec::new();
        for (token_slice, lemma_id) in analysis_result.tokens {
            let term = &analysis_result.terms[&lemma_id];
            let (spelling, reading) = term.surface_form();
            let spelling = match spelling.split_once('-') {
                Some((s, _)) => s,
                None => spelling,
            };
            ret.push(AnnToken {
                token: token_slice,
                lemma_spelling: spelling.to_string(),
                lemma_reading: reading.unwrap_or(spelling).to_string(),
                spelling: term.orth_form.clone(),
                reading: term
                    .kana_repr
                    .as_ref()
                    .unwrap_or(&term.orth_form)
                    .to_owned(),
            })
        }
        Ok(AnnTokens(ret))
    }
}

// Check that we can parse everything that's actually in Unidic.
#[test]
fn unidic_csv_parse() {
    UnidicSession::with_terms(|_| Ok(())).unwrap();
}

// Check that the weirdness of the CSV-parsing adjustments doesn't
// break our ability to roundtrip to json and back.
#[test]
fn unidic_csv_roundtrip_json() {
    UnidicSession::with_terms(|term| {
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
    .unwrap();
}
