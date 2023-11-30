#![allow(dead_code)]
use std::fs::File;

use snafu::{prelude::*, Whatever};
use tracing::debug;
use vaporetto::{Model, Predictor, Sentence, WordWeightRecord};

pub fn run() -> Result<(), Whatever> {
    debug!("starting");
    // unusably slow without the buffer
    // let reader = BufReader::new(
    //     File::open("input/kana.model").whatever_context("failed to open model file")?,
    // );
    let reader = zstd::Decoder::new(
        File::open("input/kana.model.zst").whatever_context("failed to open model file")?,
    )
    .whatever_context("zstd broke")?;
    debug!("opened file");
    let mut model = Model::read(reader).whatever_context("failed to read model")?;
    let mut dict = model.dictionary().to_vec();
    let mut extra = vec![WordWeightRecord::new(
        "火星猫".to_owned(),
        vec![0, -10000, -10000, 10000],
        "カセイネコ".to_owned(),
    )
    .unwrap()];
    dict.append(&mut extra);
    model.replace_dictionary(dict);

    debug!("read model");
    debug!("tag model count: {}", model.tag_models().len());
    let predictor = Predictor::new(model, true).whatever_context("failed to create predictor")?;

    let mut buf = String::new();

    let mut s = Sentence::default();

    s.update_raw("まぁ社長は火星猫だ。中途半端することがないように。できなかったら？")
        .whatever_context("sentence update")?;
    predictor.predict(&mut s);
    debug!("predicted");
    s.fill_tags();
    debug!("tagged");
    s.write_tokenized_text(&mut buf);

    debug!(buf);

    Ok(())
}
