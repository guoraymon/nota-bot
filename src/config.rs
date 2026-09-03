use std::collections::HashMap;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub app_id: String,
    pub client_secret: String,
    default_provider: String,
    default_model: String,
    pub providers: HashMap<String, ProviderConfig>,
}

#[derive(Deserialize)]
pub struct ProviderConfig {
    pub url: String,
    pub key: String,
    pub models: HashMap<String, ModelConfig>,
}

#[derive(Deserialize)]
pub struct ModelConfig {
    pub model: String,
    pub price: Option<ModelPrice>,
}

#[derive(Deserialize)]
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
}
