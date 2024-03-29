use regex::Regex;

lazy_static::lazy_static! {
    pub static ref KANJI_REGEX: Regex = Regex::new(r"\p{Unified_Ideograph}").expect("failed to build kanji regex");
    pub static ref ALL_JA_REGEX: Regex =
        Regex::new(r"^[○◯々-〇〻ぁ-ゖゝ-ゞァ-ヺーｦ-ﾝ\p{Radical}\p{Unified_Ideograph}]+$",)
          .expect("failed to build character counting regex");
}

pub const HIRA_START: char = '\u{3041}';
pub const HIRA_END: char = '\u{309F}';
pub const KATA_START: char = '\u{30A1}';
pub const KATA_END: char = '\u{30FF}';
pub const KATA_SHIFTABLE_START: char = '\u{30A1}';
pub const KATA_SHIFTABLE_END: char = '\u{30F6}';

// Note that we can make this function cheaper by constructing a few newtypes
// and making use of some invariants.
// For instance, the kanjidic readings are preprocessed to all be hiragana
// (we may in future change this so on is kata etc)
pub fn kata_to_hira(c: char) -> char {
    if KATA_SHIFTABLE_START <= c && c <= KATA_SHIFTABLE_END {
        let z = c as u32 + HIRA_START as u32 - KATA_START as u32;
        char::from_u32(z).expect(&format!("impossible: not katakana: {}", c))
    } else {
        c
    }
}

pub fn kata_to_hira_str(c: &str) -> String {
    c.chars().into_iter().map(kata_to_hira).collect()
}

#[inline]
pub fn is_kanji(c: char) -> bool {
    // most kanji are 3 bytes long, but not all
    // e.g. U+27614 (𧘔)
    let mut buf = [0; 4];
    let s = c.encode_utf8(&mut buf);
    KANJI_REGEX.is_match(s)
}
