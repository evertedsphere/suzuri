use anyhow::{Context, Result};
use tracing::{debug, error, instrument};

mod types;

use crate::tokeniser::{Blob, Cache, Dict};
pub use types::{Term, Unknown};

fn open_blob(s: &str) -> Result<crate::tokeniser::Blob> {
    Blob::open(&format!("data/system/unidic-cwj-3.1.0/{}", s))
        .context(format!("loading blob file {s}"))
}

#[instrument]
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

impl UnidicSession {
    pub fn new() -> Result<Self> {
        let dict = load_mecab_dict().context("loading unidic")?;
        let cache = crate::tokeniser::Cache::new();
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

    pub fn tokenize_with_cache<'a>(&mut self, input: &'a str) -> Result<Vec<(&'a str, Term)>> {
        let ret = Vec::new();
        let mut tokens = Vec::new();

        let cost = self
            .dict
            .tokenize_with_cache(&mut self.cache, input, &mut tokens)?;
        debug!(cost, "parsed");

        for token in &tokens {
            let features_raw = token.get_feature(&self.dict);
            let rec = Self::de_to_record(features_raw.as_bytes())?;
            if let Ok(term) = rec.deserialize::<Term>(None) {
                println!("{} > {:?}\n", token.get_text(&input), term);
            } else if let Ok(unk) = rec.deserialize::<Unknown>(None) {
                println!("{} > {:?}\n", token.get_text(&input), unk);
            } else {
                error!("unk: {}", features_raw);
            }
        }

        Ok(ret)
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
