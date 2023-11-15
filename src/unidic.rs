use anyhow::{Context, Result};
use serde::Deserialize;
use tracing::{debug, instrument};

fn open_blob(s: &str) -> Result<notmecab::Blob> {
    notmecab::Blob::open(&format!("data/system/unidic-cwj-3.1.0/{}", s))
        .context(format!("loading blob file {s}"))
}

#[instrument]
fn load_mecab_dict() -> Result<notmecab::Dict> {
    let sysdic = open_blob("sys.dic")?;
    let unkdic = open_blob("unk.dic")?;
    let matrix = open_blob("matrix.bin")?;
    let charbin = open_blob("char.bin")?;
    let dict = notmecab::Dict::load(sysdic, unkdic, matrix, charbin)
        .map_err(anyhow::Error::msg)
        .context("loading dict")?;
    Ok(dict)
}

/// See, e.g. https://users.rust-lang.org/t/serde-csv-empty-fields-are-the-string-null/31260/4
fn skip_unidic_empty<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    if &s == "*" {
        Ok(None)
    } else {
        Ok(Some(s))
    }
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum AccentType {
    Numeric(u64),
    General(String),
}

#[derive(Deserialize, Debug)]
pub enum MainPos {
    /// Noun
    #[serde(rename = "名詞")]
    Meishi,

    /// Verb
    #[serde(rename = "動詞")]
    Doushi,

    /// Adverb
    #[serde(rename = "副詞")]
    Fukushi,

    /// Bound auxiliary, e.g. た in 超えていた
    #[serde(rename = "助動詞")]
    Jodoushi,

    /// Particle
    #[serde(rename = "助詞")]
    Joshi,

    /// i-adjective
    #[serde(rename = "形容詞")]
    Keiyoushi,

    /// Pre-noun adjective
    #[serde(rename = "連体詞")]
    Rentaishi,

    /// Suffix
    #[serde(rename = "接尾辞")]
    Setsubiji,

    /// Punctuation
    #[serde(rename = "補助記号")]
    Hojokigou,

    #[serde(rename = "*")]
    Unspecified,

    /// Catch-all
    #[serde(untagged)]
    Other(String),
}

#[derive(Deserialize, Debug)]
pub enum SubPos {
    #[serde(rename = "一般")]
    Ippan,

    #[serde(rename = "句点")]
    Kuten,

    #[serde(rename = "読点")]
    Touten,

    #[serde(rename = "非自立可能")]
    Hijiritsukanou,

    #[serde(rename = "普通名詞")]
    Futsuumeishi,

    #[serde(rename = "係助詞")]
    Keijoshi,

    #[serde(rename = "格助詞")]
    Kakujoshi,

    /// "Name-like".
    ///
    /// 家 as a suffix is a 名詞的接尾辞.
    #[serde(rename = "名詞的")]
    Meishiteki,

    #[serde(rename = "*")]
    Unspecified,

    /// Catch-all
    #[serde(untagged)]
    Other(String),
}

#[derive(Deserialize, Debug)]
pub enum TertiaryPos {
    #[serde(rename = "一般")]
    Ippan,

    #[serde(rename = "*")]
    Unspecified,

    /// Catch-all
    #[serde(untagged)]
    Other(String),
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub struct Unk {
    /// Most general part of speech.
    ///
    /// "pos1" in Unidic 'dicrc' file.
    main_pos: MainPos,
}

/// A feature vector from a Unidic lookup.
///
/// https://pypi.org/project/unidic/
/// https://clrd.ninjal.ac.jp/unidic/faq.html
#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub struct Term {
    /// Most general part of speech.
    ///
    /// "pos1" in Unidic 'dicrc' file.
    main_pos: MainPos,

    /// "pos2" in Unidic 'dicrc' file.
    sub_pos: SubPos,

    /// "pos3" in Unidic 'dicrc' file.
    tertiary_pos: TertiaryPos,

    /// "pos4" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    pos_4: Option<String>,

    /// Conjugation type.
    ///
    /// "cType" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    conj_type: Option<String>,

    /// Conjugation form.
    /// "cForm" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    conj_form: Option<String>,

    /// "lForm" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    lemma_kata_rdg: Option<String>,

    /// "lemma" in Unidic 'dicrc' file.
    lemma: String,

    /// "orth" in Unidic 'dicrc' file.
    orth_form: String,

    /// "pron" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    pron: Option<String>,

    /// "orthBase" in Unidic 'dicrc' file.
    orth_base: String,

    /// "pronBase" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    pron_base: Option<String>,

    /// 語種, word type/etymological category.
    /// In order of frequency, 和, 固, 漢, 外, 混, 記号, 不明.
    /// Defined for all dictionary words, blank for unks.
    ///
    /// "goshu" in Unidic 'dicrc' file.
    goshu: String,

    /// "iType" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    init_trans_type: Option<String>,

    /// "iForm" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    init_form_in_ctx: Option<String>,

    /// "fType" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    final_trans_type: Option<String>,

    /// "fForm" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    final_form_in_ctx: Option<String>,

    /// "iConType" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    init_change_fusion_type: Option<String>,

    /// "fConType" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    final_change_fusion_type: Option<String>,

    /// "type" in Unidic 'dicrc' file.
    pos_type: String,

    /// "kana" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    kana_repr: Option<String>,

    /// "kanaBase" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    lemma_kana_repr: Option<String>,

    /// "form" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    form: Option<String>,

    /// "formBase" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    form_base: Option<String>,

    /// "aType" in Unidic 'dicrc' file.
    accent_type: Option<AccentType>,

    /// "aConType" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    accent_ctr_type: Option<String>,

    /// "aModType" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    accent_mod_type: Option<String>,

    /// "lid" in Unidic 'dicrc' file.
    lemma_guid: u64,

    /// "lemma_id" in Unidic 'dicrc' file.
    lemma_id: u64,
}

impl std::fmt::Display for Term {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "[{} ({:?}) (lemma: {} ({:?}); pos: {:?} ({:?}, {:?}, {:?}))]",
            self.orth_form,
            self.kana_repr,
            self.lemma,
            self.lemma_kana_repr,
            self.main_pos,
            self.sub_pos,
            self.tertiary_pos,
            self.pos_4
        )
        // Self::Unknown { main_pos } => format!("unknown: {:?}", main_pos),
    }
}

pub struct UnidicSession {
    dict: notmecab::Dict,
    cache: notmecab::Cache,
}

impl UnidicSession {
    pub fn new() -> Result<Self> {
        let dict = load_mecab_dict().context("loading unidic")?;
        let cache = notmecab::Cache::new();
        Ok(Self { dict, cache })
    }

    pub fn tokenize_with_cache<'a>(&mut self, input: &'a str) -> Result<Vec<(&'a str, Term)>> {
        let mut ret = Vec::new();
        let mut tokens = Vec::new();

        let cost = self
            .dict
            .tokenize_with_cache(&mut self.cache, input, &mut tokens)?;
        debug!(cost, "parsed");

        for token in &tokens {
            let features_raw = token.get_feature(&self.dict); //.replace("*", "");
            debug!("raw feature vector: {}", features_raw);
            let rec: csv::StringRecord = csv::ReaderBuilder::new()
                .has_headers(false)
                .flexible(false)
                .from_reader(features_raw.as_bytes())
                .records()
                .next()
                .context("no feature in stream")?
                .context("deserialising feature to record")?;
            if let Ok(term) = rec.deserialize::<Term>(None) {
                debug!("{:?} > {}", token.get_text(&input), term)
            } else if let Ok(unk) = rec.deserialize::<Unk>(None) {
                debug!("unk")
            }
        }

        Ok(ret)
    }
}
