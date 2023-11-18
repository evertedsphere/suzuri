use std::io::BufRead;
use std::io::Read;

use anyhow::Context;
use anyhow::Result;
use tracing::error;
use tracing::instrument;
use tracing::warn;

use crate::HashMap;
use crate::HashSet;

use crate::tokeniser::FormatToken;

#[derive(Debug)]
pub struct UserDict {
    pub dict: HashMap<String, Vec<FormatToken>>,
    pub contains_longer: HashSet<String>,
    pub features: Vec<String>,
}

impl UserDict {
    #[instrument(skip_all)]
    pub fn load_from<T: Read + BufRead>(file: &mut T) -> Result<UserDict> {
        let data = Self::read_csv(file)?;
        Self::load_data(data)
    }

    pub fn load_data(data: Vec<(String, String, FormatToken)>) -> Result<UserDict> {
        let mut dict: HashMap<String, Vec<FormatToken>> = HashMap::new();
        let mut contains_longer = HashSet::new();
        let mut features = Vec::new();

        for (surface, feature, token) in data.into_iter() {
            if let Some(list) = dict.get_mut(&surface) {
                list.push(token);
            } else {
                dict.insert(surface.clone(), vec![token]);
            }
            for (j, _) in surface.char_indices() {
                if j > 0 {
                    contains_longer.insert(surface[0..j].to_string());
                }
            }
            features.push(feature);
        }

        Ok(UserDict {
            dict,
            contains_longer,
            features,
        })
    }

    fn read_csv<T: Read + BufRead>(file: &mut T) -> Result<Vec<(String, String, FormatToken)>> {
        let mut data = Vec::new();

        for (i, line) in file.lines().enumerate() {
            let line = line.context("IO error")?;
            if line.is_empty() {
                warn!("skipping empty line");
                continue;
            }
            let parts: Vec<&str> = line.splitn(5, ',').collect();
            if parts.len() != 5 {
                error!("unreadable entry: {}", line);
                continue;
            }
            let surface = parts[0].to_string();
            let left_context = parts[1].parse::<u16>().context("numeric parse error")?;
            let right_context = parts[2].parse::<u16>().context("numeric parse error")?;
            let cost = parts[3].parse::<i64>().context("numeric parse error")?;
            let feature = parts[4].to_string();
            let token = FormatToken {
                left_context,
                right_context,
                pos: 0,
                cost,
                original_id: i as u32,
                feature_offset: i as u32,
            };
            data.push((surface, feature, token));
        }

        Ok(data)
    }

    pub fn may_contain(&self, find: &str) -> bool {
        self.contains_longer.contains(find) || self.dict.contains_key(find)
    }
    pub fn dic_get<'a>(&'a self, find: &str) -> Option<&'a Vec<FormatToken>> {
        self.dict.get(find)
    }
    pub fn feature_get(&self, offset: u32) -> &str {
        self.features
            .get(offset as usize)
            .map(|feature| feature.as_str())
            .unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::BufReader;

    #[test]
    fn test_unkchar_load() {
        let mut usrdic = BufReader::new(File::open("data/system/tokeniser/userdict.csv").unwrap());
        UserDict::load_from(&mut usrdic).unwrap();
    }
}
