use anyhow::Context;
use anyhow::Result;
use hashbrown::HashSet;
use itertools::Itertools;
use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::HashMap;
use tracing::debug;
use tracing::error;
use tracing::instrument;
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
    },
    Furi {
        kanji: char,
        yomi: String,
        dict_yomi: String,
    },
    Invalid {
        text: String,
        reading: String,
    },
    Unknown {
        text: String,
        reading: String,
    },
    // WithRendaku(Span),
    // WithOkuriganaConsumed(Span)
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Kana { kana } => write!(f, "{}", kana),
            Self::Furi {
                kanji,
                yomi,
                dict_yomi,
            } => write!(f, "{}({} = {})", kanji, yomi, dict_yomi),
            Self::Invalid { text, reading } => write!(f, "{}(* {})", text, reading),
            Self::Unknown { text, reading } => write!(f, "{}(? {})", text, reading),
        }
    }
}

// #[derive(Debug)]
// pub struct Annotation(Vec<Span>);

lazy_static! {
    static ref KANJI_REGEX: Regex = Regex::new(r"\p{Unified_Ideograph}").unwrap();
    static ref ALL_JA_REGEX: Regex =
        Regex::new(r"^[○◯々-〇〻ぁ-ゖゝ-ゞァ-ヺーｦ-ﾝ\p{Radical}\p{Unified_Ideograph}]+$",).unwrap();
}

#[inline]
fn is_kanji(c: char) -> bool {
    // most kanji are 3 bytes long, but not all
    // e.g. U+27614 (𧘔)
    let mut buf = [0; 4];
    let s = c.encode_utf8(&mut buf);
    KANJI_REGEX.is_match(s)
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

#[test]
fn annotate_simple() {
    let kd = read_kanjidic().unwrap();
    let words = vec![
        ("検討", "けんとう"),
        ("人か人", "ひとかひと"),
        ("口血", "くち"),
        ("化粧", "けしょう"),
        ("山々", "やまやま"),
        ("民主主義", "みんしゅしゅぎ"),
        ("社会形成推進基本法", "しゃかいけいせいすいしんきほんほう"),
    ];

    for (spelling, reading) in words {
        let furi = annotate(&spelling, &reading, &kd).context("failed to apply furi");
        assert!(furi.is_ok());
    }
}

const HIRA_START: char = '\u{3041}';
const HIRA_END: char = '\u{309F}';
const KATA_START: char = '\u{30A1}';
const KATA_END: char = '\u{30FF}';

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
fn kata_to_hira(c: char) -> char {
    if (KATA_START <= c && c <= KATA_END) {
        let z = c as u32 + HIRA_START as u32 - KATA_START as u32;
        char::from_u32(z).unwrap()
    } else {
        c
    }
}

fn initial_hira_eq_mod_sandhi(x: char, y: char) -> bool {
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
        | ('は', 'ば')
        | ('は', 'ぱ')
        | ('ひ', 'び')
        | ('ひ', 'ぴ')
        | ('ふ', 'ぶ')
        | ('ふ', 'ぷ')
        | ('へ', 'べ')
        | ('へ', 'ぺ')
        | ('ほ', 'ぼ')
        | ('ほ', 'ぽ') => true,
        _ => hira_eq(x, y),
    }
}

fn final_hira_eq_mod_sandhi(x: char, y: char) -> bool {
    match (x, y) {
        ('つ', 'っ') => true,
        // FIXME: restrict to situations like 学期
        ('く', 'っ') => true,
        // stem forms
        // TODO: only do this for actual verbs...
        // and at that point compute the stem instead of doing this blindly
        ('る', 'り') | ('む', 'み') | ('く', 'き') | ('す', 'し') | ('つ', 'ち') => true,
        // lol
        // ('る', 'ろ') => true,
        _ => hira_eq(x, y),
    }
}

fn hira_eq(x: char, y: char) -> bool {
    match (x, y) {
        ('*', _) => true,
        ('お', 'を') => true,
        _ => x == y,
    }
}

/// Simple non-recursive depth-first search, with some tweaks to account
/// for the generally fucked nature of ... anything to do with making
/// a computer understand this language
/// FIXME take '.' and '-' into account
// #[instrument(skip(kd))]
pub fn annotate<'a>(spelling: &'a str, reading: &'a str, kd: &'a KanjiDic) -> Result<Vec<Span>> {
    if !ALL_JA_REGEX.is_match(spelling) {
        error!("Invalid word: {} (* {})", spelling, reading);
        return Ok(vec![Span::Invalid {
            text: spelling.to_owned(),
            reading: reading.to_owned(),
        }]);
    }

    let mut history = Vec::new();
    let orth: Vec<char> = spelling.chars().collect();
    let pron: Vec<char> = reading.chars().collect();
    // kanji we have already added readings for on the current path
    let mut visited = Vec::new();
    let mut frontier = Vec::new();
    frontier.push(AnnotationState::Start);

    debug!("annotating {} (? {})", spelling, reading);

    while let Some(state) = frontier.pop() {
        let (mut orth_ix, mut pron_ix) = match state {
            AnnotationState::Start => (0, 0),
            AnnotationState::InProgress {
                orth_ix,
                pron_ix,
                node,
            } => {
                while let Some(n) = visited.last() {
                    if n == &orth_ix {
                        warn!("already-visited node, adjusting history:");
                        visited.pop();
                        history.pop();
                    } else {
                        break;
                    }
                }
                history.push(node);
                visited.push(orth_ix);
                (orth_ix, pron_ix)
            }
        };

        let prefix: String = " ".chars().cycle().take(1 + 2 * history.len()).collect();

        debug!(
            "{} visiting {}, {} at {:?}",
            prefix, orth_ix, pron_ix, history
        );

        let orth_len = orth.len();
        let pron_len = pron.len();

        let orth_end = orth_ix == orth_len;
        let pron_end = pron_ix == pron_len;

        if orth_end && pron_end {
            debug!("done: {:?}", history);
            return Ok(history);
        }

        if orth_end ^ pron_end {
            debug!(
                "{} backtracking: orth_end {}, pron_end {}",
                prefix, orth_end, pron_end
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
                error!("{} illegal 々 at start of string", prefix);
            }
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
                    let r_len = r.chars().count();

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

                // debug!(
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
                let mut ret = readings.clone().into_iter().unique().collect::<Vec<_>>();
                // unique().rev() does not work :)
                ret.reverse();
                ret
            };

            debug!(
                "{} char {} (= {}) has {} readings: {:?}",
                prefix,
                orth_char,
                eff_orth_char,
                dict_readings.len(),
                dict_readings
            );
            for (ri, reading) in dict_readings.iter().enumerate() {
                let rd_len = reading.chars().count();
                if rd_len > pron_len - pron_ix {
                    // The reading is too long to be part of this word here.
                    continue;
                }
                let candidate_reading = &pron[pron_ix..pron_ix + rd_len];
                if reading.chars().zip(candidate_reading).enumerate().all(
                    |(i, (dict_rdg, &cand_rdg))| {
                        let x = dict_rdg;
                        let y = kata_to_hira(cand_rdg);
                        if i == 0 {
                            initial_hira_eq_mod_sandhi(x, y)
                        } else if i == rd_len - 1 {
                            final_hira_eq_mod_sandhi(x, y)
                        } else {
                            hira_eq(x, y)
                        }
                    },
                ) {
                    let yomi = candidate_reading.iter().collect();
                    debug!(
                        "{} possible match: #{} = {} ({} ~ known {})",
                        prefix, ri, orth_char, yomi, reading
                    );
                    let node = Span::Furi {
                        kanji: orth_char,
                        // use what we got, not the reading from the dictionary
                        // that it's equivalent to
                        yomi,
                        dict_yomi: reading.clone(),
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
            if kata_to_hira(orth[orth_ix]) == kata_to_hira(pron[pron_ix]) {
                let node = Span::Kana { kana: orth_char };
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

    error!("Unable to annotate: {:?} (* {:?})", spelling, reading);
    Ok(vec![Span::Unknown {
        text: spelling.to_owned(),
        reading: reading.to_owned(),
    }])
}
