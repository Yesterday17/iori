use iori::cache::opendal::services::S3Config;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub s3: S3Config,
    pub showroom: ShowroomConfig,
}

#[derive(Serialize, Deserialize)]
pub struct ShowroomConfig {
    pub rooms: Vec<String>,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let file = "config.toml";
        let data = std::fs::read_to_string(file)?;
        let config = toml::from_str(&data)?;
        Ok(config)
    }
}
