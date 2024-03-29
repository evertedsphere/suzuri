use anyhow::bail;

use anyhow::Result;

use itertools::Itertools;
use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;

use std::collections::HashMap;
use std::fmt::Display;
use std::fmt::Formatter;

use tracing::trace;
use tracing::warn;

#[derive(Deserialize)]
pub struct KanjiDic(HashMap<char, Vec<String>>);

// more like
// enum _ {
//  Preconstrained { ... }
//  Postconstrained {...}
//  Suffix {...}
//  Prefix {...}
//  Normal {...}
// }

pub fn read_kanjidic() -> Result<KanjiDic> {
    let path = "data/system/readings.json";
    let text = std::fs::read_to_string(path).unwrap();
    let r = serde_json::from_str(&text).unwrap();
    Ok(r)
}

/// Yes, this doesn't take weird shit into account, I know
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Span {
    Kana {
        kana: char,
        pron_kana: char,
        match_kind: MatchKind,
    },
    Kanji {
        kanji: char,
        yomi: String,
        dict_yomi: String,
        match_kind: Vec<MatchKind>,
    },
}

impl Display for Span {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let display_kana_match =
            |f: &mut Formatter, l: &char, r: &char, m: &MatchKind| -> std::fmt::Result {
                match m {
                    MatchKind::Identical => write!(f, "{}", l)?,
                    MatchKind::Wildcard | MatchKind::LongVowelMark => write!(f, "{}/{}", l, r)?,
                    _ => write!(f, "{}/{}{}", l, m, r)?,
                };
                Ok(())
            };
        let display_kana_vector_match = |f: &mut Formatter,
                                         left: &str,
                                         right: &str,
                                         matches: &[MatchKind]|
         -> std::fmt::Result {
            for (i, ((l, r), m)) in left
                .chars()
                .zip(right.chars())
                .zip(matches.iter())
                .enumerate()
            {
                display_kana_match(f, &l, &r, m)?;
                if i != left.chars().count() - 1 {
                    write!(f, " ")?;
                }
            }
            Ok(())
        };
        match self {
            Self::Kana {
                kana,
                pron_kana,
                match_kind,
            } => {
                display_kana_match(f, kana, pron_kana, match_kind)?;
            }
            Self::Kanji {
                kanji,
                yomi,
                dict_yomi,
                match_kind,
            } => {
                write!(f, "{} (= ", kanji)?;
                display_kana_vector_match(f, yomi, dict_yomi, match_kind)?;
                write!(f, ")")?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ruby {
    /// It works
    Valid { spans: Vec<Span> },
    /// Shoudln't be parsing this
    Invalid { text: String, reading: String },
    /// Couldn't parse it
    Unknown { text: String, reading: String },
    /// There's a bug in the algorithm
    Inconsistent(Box<Ruby>),
}

impl std::fmt::Display for Ruby {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            Self::Valid { spans } => {
                write!(f, "[ ")?;
                for (i, span) in spans.iter().enumerate() {
                    write!(f, "{} ", span)?;
                    if i != spans.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, "]")?;
            }
            _ => write!(f, "{:?}", self)?,
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum AnnotationState {
    Start,
    InProgress {
        orth_ix: usize,
        pron_ix: usize,
        node: Span,
    },
}

/// Simple non-recursive depth-first search, with some tweaks to account
/// for the generally fucked nature of ... anything to do with making
/// a computer understand this language
pub fn annotate<'a>(spelling: &'a str, reading: &'a str, kd: &'a KanjiDic) -> Result<Ruby> {
    if !ALL_JA_REGEX.is_match(spelling) {
        trace!("Invalid word: {} (* {})", spelling, reading);
        return Ok(Ruby::Invalid {
            text: spelling.to_owned(),
            reading: reading.to_owned(),
        });
    }

    let mut history = Vec::new();
    let orth: Vec<char> = spelling.chars().collect();
    let pron: Vec<char> = reading.chars().collect();
    // kanji we have already added readings for on the current path
    let mut visited: Vec<usize> = Vec::new();
    let mut frontier = Vec::new();
    frontier.push(AnnotationState::Start);

    let mut valid_parse = None;

    trace!("annotating {} (? {})", spelling, reading);

    while let Some(state) = frontier.pop() {
        let (orth_ix, pron_ix) = match state {
            AnnotationState::Start => (0, 0),
            AnnotationState::InProgress {
                orth_ix,
                pron_ix,
                node,
            } => {
                if let Some(p) = visited.iter().position(|v| v == &orth_ix) {
                    // delete everything after the last time we were at this character
                    visited.truncate(p);
                    history.truncate(p);
                }
                history.push(node);
                visited.push(orth_ix);
                (orth_ix, pron_ix)
            }
        };

        let prefix: String = " ".chars().cycle().take(1 + 2 * history.len()).collect();

        trace!(
            "{} visiting {}, {} at {}",
            prefix,
            orth_ix,
            pron_ix,
            Ruby::Valid {
                spans: history.clone()
            }
        );

        let orth_len = orth.len();
        let pron_len = pron.len();

        let orth_end = orth_ix == orth_len;
        let pron_end = pron_ix == pron_len;

        if orth_end && pron_end {
            trace!(
                "done: {}",
                Ruby::Valid {
                    spans: history.clone()
                }
            );
            valid_parse = Some(Ruby::Valid {
                spans: history.clone(),
            });
            break;
        }

        if orth_end ^ pron_end {
            trace!(
                "{} backtracking: orth_end {}, pron_end {}",
                prefix,
                orth_end,
                pron_end
            );
            history.pop();
            visited.pop();
            continue;
        }

        let orth_char = orth[orth_ix];

        // Handling repetition marks while not returning furigana on a different
        // spelling requires that we distinguish these.
        let eff_orth_char = if orth_char == '々' {
            if orth_ix == 0 {
                bail!("{} illegal 々 at start of string", prefix);
            }
            // TODO validate that it's after a kanji
            orth[orth_ix - 1]
        } else if orth_char == 'ゝ' {
            if orth_ix == 0 {
                bail!("{} illegal ゝ at start of string", prefix);
            }
            warn!("kana iteration mark");
            // TODO validate that it's after a kana
            orth[orth_ix - 1]
        } else {
            orth_char
        };

        let mut any_next = false;

        if is_kanji(eff_orth_char) {
            let dict_readings = {
                let fallback = Vec::new();
                let raw_readings = {
                    match kd.0.get(&eff_orth_char) {
                        Some(rs) => rs,
                        None => {
                            warn!("unknown kanji: {}", eff_orth_char);
                            &fallback
                        }
                    }
                };
                let mut readings = Vec::new();
                let mut extra_readings = Vec::new();
                let mut wildcard_readings = Vec::new();
                for r in raw_readings {
                    let _r_len = r.chars().count();

                    // HACK: to be removed when we have proper handling of affix markers
                    let clean: String = r.chars().filter(|&x| x != '-' && x != '.').collect();

                    // try to find a verb stem
                    // TODO: only do this for actual verbs...
                    // and at that point compute the stem instead of doing this blindly
                    if clean.chars().last().unwrap() == 'る' {
                        let stem = clean.chars().take_while(|&x| x != 'る').collect();
                        extra_readings.push(stem);
                    }
                    readings.push(clean);

                    // Is there okurigana?
                    if let Some(p) = r.chars().position(|x| x == '.') {
                        // Take into account cases where characters "swallow"
                        // some or all of their okuri.
                        let mut s = String::new();
                        for (i, c) in r.chars().enumerate() {
                            // HACK: to be removed when we have proper handling of affix markers
                            // prefix case: prevents adding an empty reading at the start
                            // suffix case: prevents adding a duplicate reading at the end
                            if c == '-' {
                                continue;
                            }
                            if i != p {
                                // don't want the '.' itself
                                s.push(c);
                            }
                            if i >= p {
                                extra_readings.push(s.clone());
                            }
                        }
                    }
                }
                // Add wildcards
                // let mut s = '*';

                for i in 1..=pron_len {
                    let wildcard: String = "*".chars().cycle().take(i).collect();
                    wildcard_readings.push(wildcard);
                }

                // trace!(
                //     "{} de-okuri, started with {:?}, added {} readings: {:?}",
                //     prefix,
                //     raw_readings,
                //     extra_readings.len(),
                //     extra_readings
                // );

                // this orders the frontier to prefer, in order:
                // longest known readings, shorter known readings, wildcards
                readings.sort_by(|x, y| x.chars().count().cmp(&y.chars().count()).reverse());
                extra_readings.sort_by(|x, y| x.chars().count().cmp(&y.chars().count()).reverse());
                readings.append(&mut extra_readings);
                readings.append(&mut wildcard_readings);

                // stable unique, with the extra ones at the end
                let ret = readings.clone().into_iter().unique().collect::<Vec<_>>();
                // unique().rev() does not work :)
                ret
            };

            trace!(
                "{} char {} (= {}) has {} readings: {:?}",
                prefix,
                orth_char,
                eff_orth_char,
                dict_readings.len(),
                dict_readings
            );
            for (ri, reading) in dict_readings.iter().rev().enumerate() {
                let rd_len = reading.chars().count();
                if rd_len > pron_len - pron_ix {
                    // The reading is too long to be part of this word here.
                    continue;
                }
                let candidate_reading = &pron[pron_ix..pron_ix + rd_len];
                if let Some(c) = candidate_reading.get(0) {
                    match c {
                        'っ' | 'ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ' | 'ゃ' | 'ゅ' | 'ょ' => {
                            trace!("skipping candidate reading starting with character {c}");
                            continue;
                        }
                        _ => {}
                    }
                }
                if let Some(match_kind) = reading
                    .chars()
                    .zip(candidate_reading)
                    .enumerate()
                    .map(|(i, (dict_rdg, &cand_rdg))| {
                        let x = dict_rdg;
                        let y = kata_to_hira(cand_rdg);
                        if i == 0 {
                            initial_hira_eq(x, y)
                        } else if i == rd_len - 1 {
                            final_hira_eq(x, y)
                        } else {
                            hira_eq(x, y)
                        }
                    })
                    .collect::<Option<Vec<_>>>()
                {
                    let yomi = candidate_reading.iter().collect();
                    trace!(
                        "{} possible match: #{} = {} ({} ~ known {})",
                        prefix,
                        ri,
                        orth_char,
                        yomi,
                        reading
                    );
                    let node = Span::Kanji {
                        kanji: orth_char,
                        // use what we got, not the reading from the dictionary
                        // that it's equivalent to
                        yomi,
                        dict_yomi: reading.clone(),
                        match_kind,
                    };
                    frontier.push(AnnotationState::InProgress {
                        orth_ix: orth_ix + 1,
                        pron_ix: pron_ix + rd_len,
                        node,
                    });
                    any_next = true;
                }
            }
        } else {
            let orth_kana = kata_to_hira(eff_orth_char);
            let pron_kana = kata_to_hira(pron[pron_ix]);
            if let Some(eq) = hira_eq(orth_kana, pron_kana) {
                let node = Span::Kana {
                    kana: orth_char,
                    pron_kana,
                    match_kind: eq,
                };
                frontier.push(AnnotationState::InProgress {
                    orth_ix: orth_ix + 1,
                    pron_ix: pron_ix + 1,
                    node,
                });
            }
            any_next = true;
        }

        // we didn't add any new edges out, so the next iteration is going to be a
        // sibling of this node
        // so this one doesn't belong in the history
        // on the contrary, if we added children, they would link back to us
        if !any_next {
            history.pop();
            visited.pop();
        }

        let v = frontier.iter().unique().cloned().collect();
        frontier = v;
    }

    if let Some(Ruby::Valid { spans }) = valid_parse {
        let mut s = String::new();
        for span in spans.clone() {
            match span {
                Span::Kana { kana, .. } => s.push(kana),
                Span::Kanji { kanji, .. } => s.push(kanji),
            }
        }
        if &s == spelling {
            Ok(Ruby::Valid { spans })
        } else {
            Ok(Ruby::Inconsistent(Box::new(Ruby::Valid { spans })))
        }
    } else {
        trace!("Unable to annotate: {:?} (* {:?})", spelling, reading);
        Ok(Ruby::Unknown {
            text: spelling.to_owned(),
            reading: reading.to_owned(),
        })
    }
}

#[test]
fn annotate_simple() {
    use anyhow::Context;
    let kd = read_kanjidic().unwrap();
    let words = vec![
        // normal
        ("劇場版", "げきじょうばん"),
        ("化粧", "けしょう"),
        ("山々", "やまやま"),
        // rendaku
        ("人人", "ひとびと"),
        // backtracking
        ("口血", "くち"),
        // wildcards
        ("無刀", "中二病だ"),
        ("行實", "ゆきざね"),
        // old kana substitutions
        ("煩わす", "わずらはす"),
        ("を格", "ヲカク"),
        // longer ones
        ("民主主義", "みんしゅしゅぎ"),
        (
            "循環型社会形成推進基本法",
            "じゅんかんがたしゃかいけいせいすいしんきほんほう",
        ),
    ];

    for (spelling, reading) in words {
        let furi = annotate(&spelling, reading, &kd)
            .context("failed to apply furi")
            .unwrap();
        println!("{} ({}), furi: {}", spelling, reading, furi);
    }
}

const HIRA_START: char = '\u{3041}';
// const HIRA_END: char = '\u{309F}';
const KATA_START: char = '\u{30A1}';
// const KATA_END: char = '\u{30FF}';
const KATA_SHIFTABLE_START: char = '\u{30A1}';
const KATA_SHIFTABLE_END: char = '\u{30F6}';

// skip 20
// without kata_to_hira: 10.552%
// 23.010% with only kanji
// 30.587% with kana matching on rhs
// 43.133% with full kana matching
// 84.619% with hira_eq_mod_dakuten_on_right
// 88.255% with okuri elision handling
// 89.551% with stems

// Note that we can make this function cheaper by constructing a few newtypes
// and making use of some invariants.
// For instance, the kanjidic readings are preprocessed to all be hiragana
// (we may in future change this so on is kata etc)
pub fn kata_to_hira(c: char) -> char {
    if KATA_SHIFTABLE_START <= c && c <= KATA_SHIFTABLE_END {
        let z = c as u32 + HIRA_START as u32 - KATA_START as u32;
        char::from_u32(z).unwrap()
    } else {
        c
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub enum MatchKind {
    Identical,
    Voicing,
    Glottalisation,
    Stem,
    Wildcard,
    OldKana,
    LongVowelMark,
    KatakanaGa,
}

impl std::fmt::Display for MatchKind {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            // Self::Identical => Ok(()),
            // Self::Wildcard => Ok(()),
            // Self::LongVowelMark => Ok(()),
            Self::Identical => write!(f, "="),
            Self::Wildcard => write!(f, "*"),
            Self::LongVowelMark => write!(f, "lv"),
            Self::Voicing => write!(f, "v"),
            Self::Glottalisation => write!(f, "g"),
            Self::Stem => write!(f, "stem"),
            Self::OldKana => write!(f, "ok"),
            Self::KatakanaGa => write!(f, "ga"),
        }
    }
}

fn initial_hira_eq(x: char, y: char) -> Option<MatchKind> {
    // there is a cleverer way to do this by looking at the parity of the value,
    // since dakuten and non-dakuten characters alternate for the first few rows
    // and then mod 3 for the hagyo
    match (x, y) {
        ('か', 'が')
        | ('き', 'ぎ')
        | ('く', 'ぐ')
        | ('け', 'げ')
        | ('こ', 'ご')
        | ('さ', 'ざ')
        | ('し', 'じ')
        | ('す', 'ず')
        | ('せ', 'ぜ')
        | ('そ', 'ぞ')
        | ('た', 'だ')
        | ('ち', 'ぢ')
        // | ('つ', 'っ')
        // dropping that reduces it from 84.619% to 84.617%, i suspect
        // just because of 'っつつつつつっつつつ' nonsense
        | ('つ', 'づ')
        | ('て', 'で')
        | ('と', 'ど')
        | ('は', 'ば' | 'ぱ') // voicing is not really the word i want for the p- cases
        | ('ひ', 'び' | 'ぴ')
        | ('ふ', 'ぶ' | 'ぷ')
        | ('へ', 'べ' | 'ぺ')
        | ('ほ', 'ぼ' | 'ぽ') => Some(MatchKind::Voicing),
        _ => hira_eq(x, y),
    }
}

fn final_hira_eq(x: char, y: char) -> Option<MatchKind> {
    match (x, y) {
        ('つ', 'っ') => Some(MatchKind::Glottalisation),
        // FIXME: restrict to situations like 学期
        ('く', 'っ') => Some(MatchKind::Glottalisation),
        // stem forms
        // TODO: only do this for actual verbs...
        // and at that point compute the stem instead of doing this blindly
        ('る', 'り') | ('む', 'み') | ('く', 'き') | ('す', 'し') | ('つ', 'ち') => {
            Some(MatchKind::Stem)
        }
        // lol
        // ('る', 'ろ') => true,
        _ => hira_eq(x, y),
    }
}

fn hira_eq(x: char, y: char) -> Option<MatchKind> {
    match (x, y) {
        ('*', _) => Some(MatchKind::Wildcard),
        ('お', 'を') | ('わ', 'は') | ('は', 'わ') => Some(MatchKind::OldKana),
        ('ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ' | 'あ' | 'い' | 'う' | 'え' | 'お', 'ー') => {
            Some(MatchKind::LongVowelMark)
        }
        // sometimes people use katakana ke for e.g. 桃ケ丘
        ('け', 'が') => Some(MatchKind::KatakanaGa),
        _ => {
            if x == y {
                Some(MatchKind::Identical)
            } else {
                None
            }
        }
    }
}
