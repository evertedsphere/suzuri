use figment::{
    providers::{Format, Toml},
    Figment,
};
use lazy_static::lazy_static;
use serde::Deserialize;

#[derive(Debug, PartialEq, Deserialize)]
pub struct StorageConfig {
    pub data_dir: String,
}

#[derive(Debug, PartialEq, Deserialize)]
/// Global configuration, read at startup
pub struct Config {
    pub storage: StorageConfig,
}

lazy_static! {
    pub static ref CONFIG: Config = {
        Figment::new()
            .merge(Toml::file("config.toml"))
            .extract()
            .unwrap()
    };
}
