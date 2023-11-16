use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use tracing::debug;
use tracing::error;
use tracing::warn;

#[derive(Deserialize)]
pub struct KanjiDic(HashMap<char, Vec<String>>);

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
        // debug!(y, newlen, len);
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
    // kanji are always 3 bytes long (<=?)
    let mut buf = [0; 3];
    let s = c.encode_utf8(&mut buf);
    KANJI_REGEX.is_match(s)
}

#[test]
fn annotate_simple() {
    let kd = read_kanjidic().unwrap();
    let words = vec![
        ("検討", "けんとう"),
        ("人か人", "ひとかひと"),
        // ("人人", "ひとびと"),
        ("口血", "くち"),
        // ("人", "ひとこと"),
    ];

    for (spelling, reading) in words {
        let furi = furu(&spelling, &reading, &kd).context("failed to apply furi");
        assert!(furi.is_ok());
    }
}

pub fn furu(spelling: &str, reading: &str, kd: &KanjiDic) -> Result<Vec<Span>> {
    println!("");
    println!("");
    let mut ret = Vec::new();

    // FIXME take '.' and '-' into account

    let orth = spelling.chars().collect::<Vec<_>>();
    let pron = reading.chars().collect::<Vec<_>>();

    let orth_len = orth.len();
    let pron_len = pron.len();

    // "nodes" are pairs of (0 < j < n_orth, 0 < j < n_pron)

    let mut frontier = Vec::new();
    frontier.push((0, 0, Vec::<Span>::new()));
    debug!("orth: {:?} (len {})", orth, orth_len);
    debug!("pron: {:?} (len {})", pron, pron_len);

    // use a single path
    // let mut history = Vec::new();

    while let Some((orth_ix, pron_ix, path)) = frontier.pop() {
        debug!("visiting {}, {} at {:?}", orth_ix, pron_ix, path);

        let orth_end = orth_ix == orth_len;
        let pron_end = pron_ix == pron_len;

        if orth_end && pron_end {
            debug!("done: {:?}", path);
            ret = path;
            break;
        }

        if orth_end ^ pron_end {
            warn!("backtracking: orth_end {}, pron_end {}", orth_end, pron_end);
            continue;
        }

        let orth_char = orth[orth_ix];

        if !is_kanji(orth_char) {
            // the checking is far simpler here: try to match one character
            if orth[orth_ix] == pron[pron_ix] {
                let mut new_path = path.clone();
                new_path.push(Span::Furi {
                    kanji: orth_char,
                    yomi: reading.to_string(),
                });
                frontier.push((orth_ix + 1, pron_ix + 1, new_path))
            }
        } else {
            let readings = kd.0.get(&orth_char).unwrap();
            debug!(
                "{} has {} readings: {:?}",
                orth_char,
                readings.len(),
                readings
            );
            for reading in readings {
                let rd_len = reading.chars().count();
                if rd_len > pron_len - pron_ix {
                    continue;
                }
                let candidate_slice = &pron[pron_ix..pron_ix + rd_len];
                let matches = reading.chars().zip(candidate_slice).all(|(x, &y)| x == y);
                if matches {
                    // debug!("candidate next reading: {}", reading);
                    let mut new_path = path.clone();
                    new_path.push(Span::Furi {
                        kanji: orth_char,
                        yomi: reading.to_string(),
                    });
                    frontier.push((orth_ix + 1, pron_ix + rd_len, new_path))
                }
            }
        }

        if frontier.is_empty() {
            bail!("failed to find matching")
        }
    }

    Ok(ret)
}
