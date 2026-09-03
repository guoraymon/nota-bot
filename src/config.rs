use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use tokio::fs;

#[derive(Deserialize, Serialize)]
pub struct Config {
    pub app_id: String,
    pub client_secret: String,
    default_provider: String,
    default_model: String,
    pub providers: HashMap<String, ProviderConfig>,
}

#[derive(Deserialize, Serialize)]
pub struct ProviderConfig {
    pub url: String,
    pub key: String,
    pub models: HashMap<String, ModelConfig>,
}

#[derive(Deserialize, Serialize)]
pub struct ModelConfig {
    pub model: String,
    pub price: Option<ModelPrice>,
}

#[derive(Deserialize, Serialize)]
pub struct ModelPrice {
    pub input_hit: f64,
    pub input_miss: f64,
    pub output: f64,
}

impl Config {
    pub fn get_default_provider(&self) -> Option<&ProviderConfig> {
        self.providers.get(&self.default_provider)
    }

    pub fn get_default_model(&self) -> Option<&ModelConfig> {
        self.get_default_provider()?.models.get(&self.default_model)
    }

    pub fn set_default_provider(&mut self, provider: &str) {
        self.default_provider = provider.to_owned();
    }

    pub fn set_default_model(&mut self, model: &str) {
        self.default_model = model.to_owned();
    }

    pub fn load(path: PathBuf) -> Config {
        let content = std::fs::read_to_string(path).unwrap();
        serde_json::from_str(&content).unwrap()
    }

    pub async fn save(&self, path: PathBuf) -> Result<(), String> {
        let contents =
            serde_json::to_string_pretty(&self).map_err(|e| format!("serialize failed: {e}"))?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, contents)
            .await
            .map_err(|e| format!("write failed: {e}"))?;
        fs::rename(&tmp, path)
            .await
            .map_err(|e| format!("rename failed: {e}"))?;
        Ok(())
    }
}
