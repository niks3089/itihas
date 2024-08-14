use common::config::load_config_using_env_prefix;
use serde::Deserialize;

use crate::error::ApiError;

#[derive(Deserialize, PartialEq, Debug, Clone, Default)]
pub struct ApiConfig {
    pub database_config: DatabaseConfig,
    pub env: Option<String>,
    pub metrics_port: Option<u16>,
    pub metrics_host: Option<String>,
    #[serde(default = "default_server_port")]
    pub server_port: u16,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

fn default_max_connections() -> u32 {
    100
}

fn default_server_port() -> u16 {
    4040
}

impl ApiConfig {
    pub fn get_database_url(&self) -> String {
        self.database_config
            .get(DATABASE_URL_KEY)
            .and_then(|u| u.clone().into_string())
            .ok_or(ApiError::ConfigurationError {
                msg: format!("Database connection string missing: {}", DATABASE_URL_KEY),
            })
            .unwrap()
    }
}

// Types and constants used for Figment configuration items.
pub type DatabaseConfig = figment::value::Dict;

pub const DATABASE_URL_KEY: &str = "url";

pub fn setup_config() -> ApiConfig {
    load_config_using_env_prefix("API_")
}
