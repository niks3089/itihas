use std::{sync::Arc, thread::sleep, time::Duration};

use futures::Stream;
use solana_client::{nonblocking::rpc_client::RpcClient, rpc_config::RpcBlockConfig};
use solana_sdk::commitment_config::CommitmentConfig;
use solana_transaction_status::{TransactionDetails, UiTransactionEncoding};

use crate::types::BlockInfo;

pub trait Streamer {
    fn load_block_stream(&self, latest_slot: u64) -> impl Stream<Item = BlockInfo>;
    // fn get_block(
    //     client: &RpcClient,
    //     slot: u64,
    // ) -> impl std::future::Future<Output = Option<BlockInfo>> + Send;
}

pub async fn get_genesis_hash(rpc_client: &RpcClient) -> String {
    loop {
        match rpc_client.get_genesis_hash().await {
            Ok(genesis_hash) => return genesis_hash.to_string(),
            Err(e) => {
                log::error!("Failed to fetch genesis hash: {}", e);
                sleep(Duration::from_secs(5));
            }
        }
    }
}

pub async fn fetch_block_parent_slot(rpc_client: Arc<RpcClient>, slot: u64) -> u64 {
    rpc_client
        .get_block_with_config(
            slot,
            RpcBlockConfig {
                encoding: Some(UiTransactionEncoding::Base64),
                transaction_details: Some(TransactionDetails::None),
                rewards: None,
                commitment: Some(CommitmentConfig::confirmed()),
                max_supported_transaction_version: Some(0),
            },
        )
        .await
        .unwrap()
        .parent_slot
}

pub async fn fetch_current_slot(client: &RpcClient) -> u64 {
    loop {
        match client.get_slot().await {
            Ok(slot) => return slot,
            Err(e) => {
                log::error!("Failed to fetch current slot: {}", e);
                sleep(Duration::from_secs(5));
            }
        }
    }
}
