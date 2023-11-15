use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use tracing::debug;
use tracing::error;

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
#[derive(Debug)]
pub enum Span {
    Kana { kana: char },
    Furi { kanji: char, reading: String },
}

pub fn furu(spelling: &str, reading: &str, kd: &KanjiDic) -> Result<Vec<Span>> {
    let mut r = Vec::new();

    let mut rest = reading.to_owned();

    for c in spelling.chars() {
        let re = Regex::new(r"\p{Unified_Ideograph}").unwrap();
        let is_kanji = re.is_match(&format!("{}", c));
        if is_kanji {
            match kd.0.get(&c) {
                Some(readings) => {
                    let (prefix, new_rest) = longest_prefix(&rest, readings.as_ref());
                    if prefix.is_empty() {
                        bail!("ran out of characters in reading")
                    }
                    debug!("picking prefix {}", prefix);
                    r.push(Span::Furi {
                        kanji: c,
                        reading: prefix,
                    });
                    rest = new_rest;
                    // if rest.is_empty() {
                    //     break;
                    // }
                    continue;
                }
                None => {
                    bail!("unknown kanji {}", c);
                }
            }
        }
        match rest.chars().next() {
            None => error!("empty"),
            Some(c) => {
                let (prefix, new_rest) = rest.split_at(c.len_utf8());
                debug_assert!(prefix.len() == 1);
                // assert c is a single character
                let rc = prefix.chars().next();
                if rc != Some(c) {
                    bail!("non-matching char for kana {:?} != {}", rc, c);
                } else {
                    r.push(Span::Kana { kana: c });
                    rest = new_rest.to_string();
                }
            }
        }
    }

    Ok(r)
}
// pub fn furu(spelling: &str, reading: &str, kd: &KanjiDic) -> Option<Vec<Span>> {
//     let mut r = Vec::new();

//     let mut rest = reading.to_owned();

//     for c in spelling.chars() {
//         let re = Regex::new(r"\p{Unified_Ideograph}").unwrap();
//         let is_kanji = re.is_match(&format!("{}", c));
//         if is_kanji {
//             match kd.0.get(&c) {
//                 Some(readings) => {
//                     let (prefix, new_rest) = longest_prefix(&rest, readings.as_ref());
//                     if prefix.is_empty() {
//                         return None;
//                     }
//                     r.push(Span::Furi {
//                         kanji: c,
//                         reading: prefix,
//                     });
//                     rest = new_rest;
//                     // if rest.is_empty() {
//                     //     break;
//                     // }
//                     continue;
//                 }
//                 None => {
//                     error!("unknown kanji {}", c);
//                 }
//             }
//         }
//         match rest.chars().next() {
//             None => error!("empty"),
//             Some(c) => {
//                 let (prefix, new_rest) = rest.split_at(c.len_utf8());
//                 debug_assert!(prefix.len() == 1);
//                 // assert c is a single character
//                 let rc = prefix.chars().next();
//                 if rc != Some(c) {
//                     error!("non-matching chars");
//                     return None;
//                 } else {
//                     r.push(Span::Kana { kana: c });
//                     rest = new_rest.to_string();
//                 }
//             }
//         }
//     }

//     Some(r)
// }
