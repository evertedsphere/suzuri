use serde::{Deserialize, Serialize};

/// See, e.g. https://users.rust-lang.org/t/serde-csv-empty-fields-are-the-string-null/31260/4
fn skip_unidic_empty<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match String::deserialize(deserializer).as_deref() {
        Ok("") | Ok("*") | Err(_) => Ok(None),
        Ok(s) => Ok(Some(s.to_string())),
    }
}

fn comma_separated<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Debug,
{
    let s = String::deserialize(deserializer)?;
    let xs = s
        .split(",")
        .map(|s| s.parse::<T>().unwrap())
        .collect::<Vec<_>>();
    Ok(xs)
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub enum AccentType {
    #[serde(alias = "*")]
    Unspecified,

    #[serde(untagged)]
    Unique(u8),

    #[serde(untagged)]
    Variable(String),
    // #[serde(deserialize_with = "comma_separated")]
    // Variable(Vec<u8>),
    // needs to be reworked if it is to remain serializable that way
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub enum MainPos {
    /// Noun
    #[serde(alias = "名詞")]
    Meishi,

    /// Verb
    #[serde(alias = "動詞")]
    Doushi,

    /// Adverb
    #[serde(alias = "副詞")]
    Fukushi,

    /// Bound auxiliary, e.g. た in 超えていた
    #[serde(alias = "助動詞")]
    Jodoushi,

    /// Particle
    #[serde(alias = "助詞")]
    Joshi,

    /// i-adjective
    #[serde(alias = "形容詞")]
    Keiyoushi,

    /// na-adjective
    #[serde(alias = "形状詞")]
    Keijoushi,

    /// Pre-noun adjective
    #[serde(alias = "連体詞")]
    Rentaishi,

    /// Suffix
    #[serde(alias = "接尾辞")]
    Setsubiji,

    /// Punctuation
    #[serde(alias = "補助記号")]
    Hojokigou,

    /// Punctuation
    #[serde(alias = "記号")]
    Kigou,

    /// Pronoun
    #[serde(alias = "代名詞")]
    Daimeishi,

    /// Interjection
    #[serde(alias = "感動詞")]
    Kandoushi,

    /// Suffix
    #[serde(alias = "接続詞")]
    Setsubishi,

    /// Prefix
    #[serde(alias = "接頭辞")]
    Settouji,

    /// Blank
    #[serde(alias = "空白")]
    Kuuhaku,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub enum SubPos {
    #[serde(alias = "一般")]
    Ippan,

    #[serde(alias = "句点")]
    Kuten,

    #[serde(alias = "読点")]
    Touten,

    #[serde(alias = "非自立可能")]
    Hijiritsukanou,

    #[serde(alias = "普通名詞")]
    Futsuumeishi,

    #[serde(alias = "係助詞")]
    Keijoshi,

    #[serde(alias = "格助詞")]
    Kakujoshi,

    #[serde(alias = "終助詞")]
    Shuujoshi,

    /// "Name-like".
    ///
    /// 家 as a suffix is a 名詞的接尾辞.
    #[serde(alias = "名詞的")]
    Meishiteki,

    /// "Filler"
    ///
    /// I'm keeping this in romaji purely because it's funny
    #[serde(alias = "フィラー")]
    Firaa,

    /// 形状詞-タリ 「釈然」「錚々」など、いわゆるタリ活用の形容動詞の語幹部分
    #[serde(alias = "タリ")]
    Tari,

    #[serde(alias = "ＡＡ")]
    AsciiArt,

    #[serde(alias = "*")]
    Unspecified,

    /// Catch-all
    #[serde(untagged)]
    Other(String),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub enum TertiaryPos {
    #[serde(alias = "一般")]
    Ippan,

    #[serde(alias = "*")]
    Unspecified,

    #[serde(alias = "人名")]
    Jinmei,

    /// Catch-all
    #[serde(untagged)]
    Other(String),
}

/// Only used for 固有名詞, blank otherwise
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum ExtraPos {
    #[serde(alias = "*")]
    Unspecified,

    /// Country name
    #[serde(alias = "国")]
    Kuni,

    /// "Normal"
    #[serde(alias = "一般")]
    Ippan,

    /// Personal name?
    #[serde(alias = "名")]
    Myou,

    /// Family name
    #[serde(alias = "姓")]
    Sei,
}

/// In order of frequency, 和, 固, 漢, 外, 混, 記号, 不明.
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub enum Goshu {
    /// 和語
    #[serde(alias = "和")]
    Wago,

    /// 漢 漢語
    #[serde(alias = "漢")]
    Kango,

    /// 外 外来語
    #[serde(alias = "外")]
    Gairaigo,

    /// 混 混種語
    #[serde(alias = "混")]
    Konshugo,

    /// 固 固有名
    #[serde(alias = "固")]
    Koyuumei,

    /// 記 記号
    #[serde(alias = "記号")]
    Kigou,

    /// 他 その他
    #[serde(alias = "他")]
    Hoka,

    #[serde(alias = "不明")]
    Fumei,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub enum ConjForm {
    #[serde(alias = "連用形-促音便")]
    RennyoukeiSokuonbin,

    #[serde(alias = "*")]
    Unspecified,

    #[serde(untagged)]
    Other(String),
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
#[allow(dead_code)]
pub struct Unknown {
    /// Most general part of speech.
    ///
    /// "pos1" in Unidic 'dicrc' file.
    main_pos: MainPos,

    /// "pos2" in Unidic 'dicrc' file.
    sub_pos: SubPos,

    /// "pos3" in Unidic 'dicrc' file.
    tertiary_pos: TertiaryPos,

    /// "pos4" in Unidic 'dicrc' file.
    extra_pos: ExtraPos,

    /// Conjugation type.
    ///
    /// "cType" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    conj_type: Option<String>,

    /// Conjugation form.
    /// "cForm" in Unidic 'dicrc' file.
    conj_form: ConjForm,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Hash, Copy, Clone)]
#[serde(transparent)]
pub struct LemmaGuid(pub u64);

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Hash, Copy, Clone)]
#[serde(transparent)]
pub struct LemmaId(pub u64);

/// A feature vector from a Unidic lookup.
///
/// https://pypi.org/project/unidic/
/// https://clrd.ninjal.ac.jp/unidic/faq.html
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[allow(dead_code)]
pub struct Term {
    /// Most general part of speech.
    ///
    /// "pos1" in Unidic 'dicrc' file.
    main_pos: MainPos,

    /// "pos2" in Unidic 'dicrc' file.
    sub_pos: SubPos,

    /// "pos3" in Unidic 'dicrc' file.
    pub tertiary_pos: TertiaryPos,

    /// "pos4" in Unidic 'dicrc' file.
    pub extra_pos: ExtraPos,

    /// Conjugation type.
    ///
    /// "cType" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    conj_type: Option<String>,

    /// Conjugation form.
    /// "cForm" in Unidic 'dicrc' file.
    conj_form: ConjForm,

    /// "lForm" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    pub lemma_kata_rdg: Option<String>,

    /// "lemma" in Unidic 'dicrc' file.
    pub lemma: String,

    /// "orth" in Unidic 'dicrc' file.
    pub orth_form: String,

    /// "pron" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    pub pron: Option<String>,

    /// "orthBase" in Unidic 'dicrc' file.
    pub orth_base: String,

    /// "pronBase" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    pub pron_base: Option<String>,

    /// 語種, word type/etymological category.
    /// In order of frequency, 和, 固, 漢, 外, 混, 記号, 不明.
    /// Defined for all dictionary words, blank for unks.
    ///
    /// "goshu" in Unidic 'dicrc' file.
    goshu: Goshu,

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
    pub kana_repr: Option<String>,

    /// "kanaBase" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    pub lemma_kana_repr: Option<String>,

    /// "form" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    form: Option<String>,

    /// "formBase" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    pub form_base: Option<String>,

    /// "aType" in Unidic 'dicrc' file.
    accent_type: AccentType,

    /// "aConType" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    accent_ctr_type: Option<String>,

    /// "aModType" in Unidic 'dicrc' file.
    #[serde(deserialize_with = "skip_unidic_empty")]
    accent_mod_type: Option<String>,

    /// "lid" in Unidic 'dicrc' file.
    pub lemma_guid: LemmaGuid,

    /// "lemma_id" in Unidic 'dicrc' file.
    pub lemma_id: LemmaId,
}

impl Term {
    pub fn surface_form<'a>(&'a self) -> (String, String, String, String) {
        let spelling = &self.orth_form;
        let kana_repr = self
            .kana_repr
            .as_ref()
            .unwrap_or(&self.orth_form)
            .to_owned();
        (
            self.orth_form.clone(),
            kana_repr.clone(),
            spelling.to_owned(),
            kana_repr, // reading.as_deref().unwrap_or(&spelling).to_string(),
        )
    }
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
            self.extra_pos
        )
        // Self::Unknown { main_pos } => format!("unknown: {:?}", main_pos),
    }
}
