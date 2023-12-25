use std::io::{BufRead, Read};

use snafu::ResultExt;
use tracing::{error, instrument, warn};

use crate::{FormatToken, HashMap, HashSet, Result};

#[derive(Debug)]
pub struct UserDict {
    pub dict: HashMap<String, Vec<FormatToken>>,
    pub contains_longer: HashSet<String>,
    pub features: Vec<String>,
}

pub type RawUserDict = Vec<(String, String, FormatToken)>;

impl UserDict {
    pub fn new() -> Self {
        Self {
            dict: Default::default(),
            contains_longer: Default::default(),
            features: Default::default(),
        }
    }

    pub fn build_entry(
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

    #[instrument(skip_all, level = "trace")]
    pub fn load_from<T: Read + BufRead>(&mut self, file: &mut T) -> Result<()> {
        let data = Self::read_csv(file)?;
        self.load_data(data)
    }

    pub fn load_data(&mut self, data: RawUserDict) -> Result<()> {
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
    fn read_csv<T: Read + BufRead>(file: &mut T) -> Result<RawUserDict> {
        let mut data = Vec::new();
        for (i, line) in file.lines().enumerate() {
            let line = line.whatever_context("IO error")?;
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
            let left_context = parts[1]
                .parse::<u16>()
                .whatever_context("numeric parse error")?;
            let right_context = parts[2]
                .parse::<u16>()
                .whatever_context("numeric parse error")?;
            let cost = parts[3]
                .parse::<i64>()
                .whatever_context("numeric parse error")?;
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
    use std::{fs::File, io::BufReader};

    use super::*;
    use crate::IoError;

    #[test]
    fn test_unkchar_load() -> Result<()> {
        let mut usrdic_file = BufReader::new(
            File::open("/home/s/c/szr/data/system/morph/userdict.csv").context(IoError)?,
        );
        let mut usrdic = UserDict::new();
        usrdic.load_from(&mut usrdic_file)
    }
}
