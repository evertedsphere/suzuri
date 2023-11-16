use anyhow::{Context, Result};
use hashbrown::HashMap;
use tracing::{debug, error, info, instrument, trace};

mod types;

use crate::tokeniser::{Blob, Cache, Dict};
pub use types::{Term, Unknown};

use self::types::LemmaGuid;

fn open_blob(s: &str) -> Result<crate::tokeniser::Blob> {
    Blob::open(&format!("data/system/unidic-cwj-3.1.0/{}", s))
        .context(format!("loading blob file {s}"))
}

fn load_mecab_dict() -> Result<crate::tokeniser::Dict> {
    let sysdic = open_blob("sys.dic")?;
    let unkdic = open_blob("unk.dic")?;
    let matrix = open_blob("matrix.bin")?;
    let charbin = open_blob("char.bin")?;
    let dict = Dict::load(sysdic, unkdic, matrix, charbin).context("loading dict")?;
    Ok(dict)
}

pub struct UnidicSession {
    dict: Dict,
    cache: Cache,
}

pub struct TokeniseResult<'a> {
    tokens: Vec<(&'a str, LemmaGuid)>,
    terms: HashMap<LemmaGuid, Term>,
}

impl UnidicSession {
    pub fn new() -> Result<Self> {
        let dict = load_mecab_dict().context("loading unidic")?;
        let cache = crate::tokeniser::Cache::new();
        info!("initialised unidic session");
        Ok(Self { dict, cache })
    }

    fn de_to_record<R: std::io::Read>(r: R) -> Result<csv::StringRecord> {
        let r = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(false)
            .from_reader(r)
            .records()
            .next()
            .context("no feature in stream")?
            .context("deserialising feature to record")?;
        Ok(r)
    }

    #[instrument(skip_all)]
    pub fn tokenize_with_cache<'a>(&mut self, input: &'a str) -> Result<TokeniseResult<'a>> {
        let mut tokens = Vec::new();
        let mut terms = HashMap::new();

        let mut buf = Vec::new();

        let cost = self
            .dict
            .tokenize_with_cache(&mut self.cache, input, &mut buf)?;
        let cost_per_token = cost as f32 / buf.len() as f32;
        debug!(cost, cost_per_token, "finished tokenising");

        let mut unk_count = 0;

        for token in &buf {
            let features_raw = token.get_feature(&self.dict);
            let rec = Self::de_to_record(features_raw.as_bytes())?;
            if let Ok(term) = rec.deserialize::<Term>(None) {
                let id = term.lemma_guid;
                terms.insert(id, term);
                tokens.push((token.get_text(&input), id));
            } else if let Ok(unk) = rec.deserialize::<Unknown>(None) {
                unk_count += 1;
                // FIXME fallback
                tokens.push((token.get_text(&input), LemmaGuid(0)));
                trace!("{:?} > {:?}\n", token.get_text(&input), unk);
            } else {
                error!("unk: {}", features_raw);
            }
        }

        debug!(
            "finished dumping {} tokens ({} unks, {} unique terms)",
            tokens.len(),
            unk_count,
            terms.len()
        );

        Ok(TokeniseResult { tokens, terms })
    }
}

/// Check that we can parse everything that's actually in Unidic.
#[test]
fn parse_unidic_csv() {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_path("data/system/unidic-cwj-3.1.0/lex_3_1.csv")
        .unwrap();
    for rec_full in rdr.records() {
        // the raw unidic csv contains four extra fields at the beginning
        // ideally i would be able to do serde(flatten) on a local type but
        // it's very finicky with csv apparently
        let rec_full = rec_full.unwrap();
        let mut rec = csv::StringRecord::new();
        for f in rec_full.iter().skip(4) {
            rec.push_field(f);
        }
        if let Ok(_line) = rec.deserialize::<Term>(None) {
            // do nothing
        } else {
            panic!("failed to deserialise record: {:?}", rec);
        }
    }
}
