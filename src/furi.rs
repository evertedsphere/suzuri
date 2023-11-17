use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::HashMap;
use tracing::debug;
use tracing::error;
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

fn longest_prefix(x: &str, ys: &[String]) -> (String, String) {
    if ys.is_empty() {
        return ("".to_string(), x.to_string());
    }
    let xc = x.chars();
    let mut len = 0;
    for y in ys {
        let newlen = y
            .chars()
            .zip(xc.clone())
            .take_while(|&(a, b)| a == b)
            .count();
        len = std::cmp::max(len, newlen);
        // trace!(y, newlen, len);
    }
    // lol
    let prefix = xc.clone().take(len).collect();
    let suffix = xc.skip(len).collect();
    // (xc.take(len).to_string(), xc. .to_string())
    (prefix, suffix)
}

/// Yes, this doesn't take weird shit into account, I know
#[derive(Debug, Clone)]
pub enum Span {
    Kana { kana: char },
    Furi { kanji: char, yomi: String },
}

// #[derive(Debug)]
// pub struct Annotation(Vec<Span>);

lazy_static! {
    static ref KANJI_REGEX: Regex = Regex::new(r"\p{Unified_Ideograph}").unwrap();
}

#[inline]
fn is_kanji(c: char) -> bool {
    // most kanji are 3 bytes long, but not all
    // e.g. U+27614 (𧘔)
    let mut buf = [0; 4];
    let s = c.encode_utf8(&mut buf);
    KANJI_REGEX.is_match(s)
}

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
// without this: 10.552%
// 23.010% with only kanji
// 30.587% with kanji + only kana rhs
// 43.133% with kanji + kana
fn kata_to_hira(c: char) -> char {
    if (KATA_START <= c && c <= KATA_END) {
        let z = c as u32 + HIRA_START as u32 - KATA_START as u32;
        char::from_u32(z).unwrap()
    } else {
        c
    }
}

/// Simple non-recursive depth-first search, with some tweaks to account
/// for the generally fucked nature of ... anything to do with making
/// a computer understand this language
/// FIXME take '.' and '-' into account
pub fn annotate<'a>(spelling: &'a str, reading: &'a str, kd: &'a KanjiDic) -> Result<Vec<Span>> {
    let mut history = Vec::new();
    let orth: Vec<char> = spelling.chars().collect();
    let pron: Vec<char> = reading.chars().collect();
    let mut frontier = Vec::new();
    frontier.push(AnnotationState::Start);

    while let Some(state) = frontier.pop() {
        let (mut orth_ix, mut pron_ix) = match state {
            AnnotationState::Start => (0, 0),
            AnnotationState::InProgress {
                orth_ix,
                pron_ix,
                node,
            } => {
                history.push(node);
                (orth_ix, pron_ix)
            }
        };

        trace!("visiting {}, {} at {:?}", orth_ix, pron_ix, history);

        let orth_end = orth_ix == orth.len();
        let pron_end = pron_ix == pron.len();

        if orth_end && pron_end {
            trace!("done: {:?}", history);
            return Ok(history);
        }

        if orth_end ^ pron_end {
            trace!("backtracking: orth_end {}, pron_end {}", orth_end, pron_end);
            history.pop();
            continue;
        }

        let orth_char = orth[orth_ix];

        // Handling repetition marks while not returning furigana on a different
        // spelling requires that we distinguish these.
        let eff_orth_char = if orth_char == '々' {
            if orth_ix == 0 {
                error!("illegal 々 at start of string");
            }
            orth[orth_ix - 1]
        } else {
            orth_char
        };

        if is_kanji(eff_orth_char) {
            let readings =
                kd.0.get(&eff_orth_char)
                    .context(format!("unknown kanji {}", eff_orth_char))?; //.unwrap();
            trace!(
                "{} ({}) has {} readings: {:?}",
                orth_char,
                eff_orth_char,
                readings.len(),
                readings
            );
            for reading in readings {
                let rd_len = reading.chars().count();
                if rd_len > pron.len() - pron_ix {
                    // The reading is too long to be part of this word here.
                    continue;
                }
                let candidate_reading = &pron[pron_ix..pron_ix + rd_len];
                if reading
                    .chars()
                    .zip(candidate_reading)
                    .enumerate()
                    .all(|(_i, (x, &y))| x == kata_to_hira(y))
                {
                    let node = Span::Furi {
                        kanji: orth_char,
                        // use what we got, not the reading from the dictionary
                        // that it's equivalent to
                        yomi: candidate_reading.iter().collect(),
                    };
                    orth_ix += 1;
                    pron_ix += rd_len;
                    frontier.push(AnnotationState::InProgress {
                        orth_ix,
                        pron_ix,
                        node,
                    })
                }
            }
        } else {
            if kata_to_hira(orth[orth_ix]) == kata_to_hira(pron[pron_ix]) {
                let node = Span::Kana { kana: orth_char };
                orth_ix += 1;
                pron_ix += 1;
                frontier.push(AnnotationState::InProgress {
                    orth_ix,
                    pron_ix,
                    node,
                })
            }
        }

        if frontier.is_empty() {
            error!("{:?} (* {:?}): failed to find matching", spelling, reading);
            bail!("{:?} (* {:?}): failed to find matching", spelling, reading);
        }
    }

    bail!("failed to parse");
}
