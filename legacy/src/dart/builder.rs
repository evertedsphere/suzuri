use itertools::Itertools;

use super::{DoubleArrayTrie, DoubleArrayTrieBuilder};
use std::fs::File;
use std::io::{self, BufRead, BufReader};

pub struct IndexBuilder {}

impl IndexBuilder {
    pub fn new() -> Self {
        IndexBuilder {}
    }

    // Require the dictionary to be sorted in lexicographical order
    pub fn build<R: BufRead>(&mut self, dict: &mut R) -> io::Result<DoubleArrayTrie> {
        let mut buf = String::new();
        let mut records: Vec<String> = Vec::new();
        let mut n = 0;

        while dict.read_line(&mut buf)? > 0 {
            {
                n += 1;
                // if n % 1000 == 0 {
                //     println!("iter: {n}, line: {buf:?}");
                // }
                let parts: Vec<&str> = buf.trim().split_whitespace().collect();
                if parts.is_empty() {
                    continue;
                }

                let word = parts[0].trim_end();
                // let freq = parts
                //     .get(1)
                //     .map(|x| x.parse::<usize>().unwrap())
                //     .unwrap_or(0);
                // let tag = parts.get(2).cloned().unwrap_or("");

                records.push(String::from(word));
            }
            buf.clear();
        }

        records.sort();

        let records = records.into_iter().unique().collect::<Vec<String>>();

        let mut prev = String::new();

        for s in records.iter() {
            if s > &prev {
            } else {
                assert!(s > &prev, "lex: {:?} > {:?} failed", s, prev);
            }
            prev = s.clone();
        }

        let strs: Vec<&str> = records.iter().map(|n| n.as_ref()).collect();
        let da = DoubleArrayTrieBuilder::new().build(&strs);

        Ok(da)
    }
}

// fn main() {
//     let f = File::open("./dict.txt").unwrap();
//     let mut buf = BufReader::new(f);
//     let _ = IndexBuilder::new().build(&mut buf).unwrap();
// }
