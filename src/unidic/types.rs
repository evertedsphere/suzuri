use serde::Deserialize;

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

#[derive(Deserialize, Debug)]
pub enum AccentType {
    #[serde(rename = "*")]
    Unspecified,

    #[serde(untagged)]
    Unique(u8),

    #[serde(untagged)]
    #[serde(deserialize_with = "comma_separated")]
    Variable(Vec<u8>),
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

    /// na-adjective
    #[serde(rename = "形状詞")]
    Keijoushi,

    /// Pre-noun adjective
    #[serde(rename = "連体詞")]
    Rentaishi,

    /// Suffix
    #[serde(rename = "接尾辞")]
    Setsubiji,

    /// Punctuation
    #[serde(rename = "補助記号")]
    Hojokigou,

    /// Punctuation
    #[serde(rename = "記号")]
    Kigou,

    /// Pronoun
    #[serde(rename = "代名詞")]
    Daimeishi,

    /// Interjection
    #[serde(rename = "感動詞")]
    Kandoushi,

    /// Suffix
    #[serde(rename = "接続詞")]
    Setsubishi,

    /// Prefix
    #[serde(rename = "接頭辞")]
    Settouji,

    /// Blank
    #[serde(rename = "空白")]
    Kuuhaku,
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

    #[serde(rename = "終助詞")]
    Shuujoshi,

    /// "Name-like".
    ///
    /// 家 as a suffix is a 名詞的接尾辞.
    #[serde(rename = "名詞的")]
    Meishiteki,

    /// "Filler"
    ///
    /// I'm keeping this in romaji purely because it's funny
    #[serde(rename = "フィラー")]
    Firaa,

    /// 形状詞-タリ 「釈然」「錚々」など、いわゆるタリ活用の形容動詞の語幹部分
    #[serde(rename = "タリ")]
    Tari,

    #[serde(rename = "ＡＡ")]
    AsciiArt,

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

/// Only used for 固有名詞, blank otherwise
#[derive(Deserialize, Debug)]
pub enum ExtraPos {
    #[serde(rename = "*")]
    Unspecified,

    /// Country name
    #[serde(rename = "国")]
    Kuni,

    /// "Normal"
    #[serde(rename = "一般")]
    Ippan,

    /// Personal name?
    #[serde(rename = "名")]
    Myou,

    /// Family name
    #[serde(rename = "姓")]
    Sei,
}

/// In order of frequency, 和, 固, 漢, 外, 混, 記号, 不明.
#[derive(Deserialize, Debug)]
pub enum Goshu {
    /// 和語
    #[serde(rename = "和")]
    Wago,

    /// 漢 漢語
    #[serde(rename = "漢")]
    Kango,

    /// 外 外来語
    #[serde(rename = "外")]
    Gairaigo,

    /// 混 混種語
    #[serde(rename = "混")]
    Konshugo,

    /// 固 固有名
    #[serde(rename = "固")]
    Koyuumei,

    /// 記 記号
    #[serde(rename = "記号")]
    Kigou,

    /// 他 その他
    #[serde(rename = "他")]
    Hoka,

    #[serde(rename = "不明")]
    Fumei,
}

#[derive(Deserialize, Debug)]
pub enum ConjForm {
    #[serde(rename = "連用形-促音便")]
    RennyoukeiSokuonbin,

    #[serde(rename = "*")]
    Unspecified,

    #[serde(untagged)]
    Other(String),
}

#[derive(Deserialize, Debug)]
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

#[derive(Deserialize, Debug, Eq, PartialEq, Hash, Copy, Clone)]
#[serde(transparent)]
pub struct LemmaGuid(pub u64);

#[derive(Deserialize, Debug)]
#[serde(transparent)]
pub struct LemmaId(u64);

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
    extra_pos: ExtraPos,

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
    lemma_id: LemmaId,
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
