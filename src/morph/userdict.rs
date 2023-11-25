use std::io::BufRead;
use std::io::Read;

use anyhow::Context;
use anyhow::Result;

use tracing::error;
use tracing::instrument;
use tracing::warn;

use crate::HashMap;
use crate::HashSet;

use crate::morph::FormatToken;

#[derive(Debug)]
pub struct UserDict {
    pub dict: HashMap<String, Vec<FormatToken>>,
    pub contains_longer: HashSet<String>,
    pub features: Vec<String>,
}

const NAME_COST: i64 = -2000;

const SEI_LEFT: u16 = 2793;
const SEI_RIGHT: u16 = 11570;
const SEI_POS: &'static str = "名詞,普通名詞,人名,姓";

const MYOU_LEFT: u16 = 357;
const MYOU_RIGHT: u16 = 14993;
const MYOU_POS: &'static str = "名詞,普通名詞,人名,名";

pub enum NameType {
    Myou,
    Sei,
}

impl UserDict {
    pub fn new() -> Self {
        Self {
            dict: Default::default(),
            contains_longer: Default::default(),
            features: Default::default(),
        }
    }

    pub fn load_names(&mut self, data: Vec<(NameType, &str, &str)>) -> Result<()> {
        let mut r = Vec::new();
        let mut i = self.features.len() as u32;
        for (name_type, surface, kata_rdg) in data.into_iter() {
            let (pos, left, right) = match name_type {
                NameType::Myou => (MYOU_POS, MYOU_LEFT, MYOU_RIGHT),
                NameType::Sei => (SEI_POS, SEI_LEFT, SEI_RIGHT),
            };

            let feature = Self::build_unidic_feature_string(400_000 + i, pos, &surface, &kata_rdg);
            let entry = Self::build_entry(left, right, NAME_COST, i, &surface, &feature);
            r.push(entry);
            i += 1;
        }
        self.load_data(r)?;
        Ok(())
    }

    fn build_unidic_feature_string(
        id: u32,
        pos_str: &str,
        surface: &str,
        kata_rdg: &str,
    ) -> String {
        format!("{pos_str},*,*,{kata_rdg},{surface},{surface},{kata_rdg},{surface},{kata_rdg},漢,*,*,*,*,*,*,体,{kata_rdg},{kata_rdg},{kata_rdg},{kata_rdg},*,*,*,{id},{id}")
    }

    fn build_entry(
        left_context: u16,
        right_context: u16,
        cost: i64,
        id: u32,
        surface: &str,
        feature: &str,
    ) -> (String, String, FormatToken) {
        let token = FormatToken {
            left_context,
            right_context,
            pos: 0,
            cost,
            original_id: id,
            feature_offset: id,
        };
        (surface.to_owned(), feature.to_owned(), token)
    }

    #[instrument(skip_all)]
    pub fn load_from<T: Read + BufRead>(&mut self, file: &mut T) -> Result<()> {
        let data = Self::read_csv(file)?;
        self.load_data(data)
    }

    pub fn load_data(&mut self, data: Vec<(String, String, FormatToken)>) -> Result<()> {
        for (surface, feature, token) in data.into_iter() {
            if let Some(list) = self.dict.get_mut(&surface) {
                list.push(token);
            } else {
                self.dict.insert(surface.clone(), vec![token]);
            }
            for (j, _) in surface.char_indices() {
                if j > 0 {
                    self.contains_longer.insert(surface[0..j].to_string());
                }
            }
            self.features.push(feature);
        }
        Ok(())
    }

    // FIXME handle features.len () + i etc
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
            data.push(Self::build_entry(
                left_context,
                right_context,
                cost,
                i as u32,
                &surface,
                &feature,
            ));
        }

        Ok(data)
    }

    pub fn may_contain(&self, find: &str) -> bool {
        self.contains_longer.contains(find) || self.dict.contains_key(find)
    }
    pub fn dic_get<'a>(&'a self, find: &str) -> Option<&'a Vec<FormatToken>> {
        self.dict.get(find)
    }
    pub fn feature_get(&self, offset: u32) -> Option<&str> {
        self.features
            .get(offset as usize)
            .map(|feature| feature.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::BufReader;

    #[test]
    fn test_unkchar_load() {
        let mut usrdic_file = BufReader::new(File::open("data/system/morph/userdict.csv").unwrap());
        let mut usrdic = UserDict::new();
        usrdic.load_from(&mut usrdic_file).unwrap();
    }
}
