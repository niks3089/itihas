use common::config::load_config_using_env_prefix;
use serde::Deserialize;

use crate::error::IndexerError;

#[derive(Deserialize, PartialEq, Debug, Clone, Default)]
pub struct IndexerConfig {
    pub database_config: DatabaseConfig,
    pub env: Option<String>,
    pub rpc_config: RpcConfig,
    pub max_connections: Option<u32>,
    pub account_stream_worker_count: Option<u32>,
    pub max_concurrent_block_fetches: Option<usize>,
    pub grpc_url: Option<String>,
    #[serde(default = "default_start_slot")]
    pub start_slot: u64,
    #[serde(default = "default_workers")]
    pub workers: u16,
    pub index_recent: Option<bool>,
}

fn default_workers() -> u16 {
    100
}

fn default_start_slot() -> u64 {
    0
}

impl IndexerConfig {
    pub fn get_database_url(&self) -> String {
        self.database_config
            .get(DATABASE_URL_KEY)
            .and_then(|u| u.clone().into_string())
            .ok_or(IndexerError::ConfigurationError {
                msg: format!("Database connection string missing: {}", DATABASE_URL_KEY),
            })
            .unwrap()
    }

    pub fn get_rpc_url(&self) -> String {
        self.rpc_config
            .get(RPC_URL_KEY)
            .and_then(|u| u.clone().into_string())
            .ok_or(IndexerError::ConfigurationError {
                msg: format!("RPC connection string missing: {}", RPC_URL_KEY),
            })
            .unwrap()
    }

    pub fn get_account_stream_worker_count(&self) -> u32 {
        self.account_stream_worker_count.unwrap_or(2)
    }
}

// Types and constants used for Figment configuration items.
pub type DatabaseConfig = figment::value::Dict;

pub const DATABASE_URL_KEY: &str = "url";
pub const DATABASE_LISTENER_CHANNEL_KEY: &str = "listener_channel";
pub const RPC_URL_KEY: &str = "url";

pub type RpcConfig = figment::value::Dict;

pub fn setup_config() -> IndexerConfig {
    load_config_using_env_prefix("INDEXER_")
}
