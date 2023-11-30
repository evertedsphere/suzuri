#![allow(dead_code)]
use std::{collections::HashMap, fs::File, io::BufReader};

use snafu::{prelude::*, Whatever};
use tracing::{debug, warn};
use vaporetto::{Model, Predictor, Sentence};

#[derive(Debug, Clone)]
pub struct DictItem<P, S> {
    pub item: S,
    pub pos: P,
}

#[derive(Debug, Clone)]
pub struct Dictionary<P, S = String>(pub Vec<DictItem<P, S>>);

#[derive(Debug, Clone)]
pub struct AnnToken<P, W> {
    content: W,
    pos: P,
}

#[derive(Debug, Clone)]
pub struct TrainItem<P, S, W> {
    text: S,
    tokens: Vec<AnnToken<P, W>>,
}

#[derive(Debug, Clone)]
pub struct TrainingSet<P, S = String, W = Vec<char>>(Vec<TrainItem<S, W, P>>);

#[derive(Debug, Clone)]
pub struct PatternKey<P, C> {
    seq: Vec<C>,
    pos: Option<P>,
}

#[derive(Debug, Clone)]
pub struct PatternSet<P, C = char, V = u32>(HashMap<PatternKey<C, P>, V>);

impl<P, C, V> Default for PatternSet<P, C, V> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

/// Algorithm:
/// init P^ to empty
/// L_max = max word length in dictionary
/// for each trainitem:
///   set offset = 0
///   for j = 0 to #words in given parse:
///     shift = len of w_j
///     for k = shift to L_max do
///       p^[c{i..=i+k}][shift, t_j] += 1, ditto with t_j-1
///     offset += shift
pub fn run(dict: Dictionary<String>) -> Result<PatternSet<String>, Whatever> {
    warn!("running tokeniser");

    let p_hat = Default::default();

    Ok(p_hat)
}
