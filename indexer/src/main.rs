use common::{db::setup_database_connection, init_logger};
use log::{error, info};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;
use std::{sync::Arc, time::Duration};

use indexer::{
    config::setup_config,
    db::Dao,
    error::IndexerError,
    grpc::GrpcStreamer,
    messenger,
    poller::PollerStreamer,
    streamer::{continously_index_new_blocks, fetch_block_parent_slot, Streamer},
    types::BlockStreamConfig,
};

pub mod error;

#[tokio::main(flavor = "multi_thread")]
pub async fn main() -> Result<(), IndexerError> {
    init_logger();

    let config = setup_config();
    let dao = Dao::new(setup_database_connection(config.get_database_url(), 10).await);

    let rpc_client = Arc::new(RpcClient::new_with_timeout_and_commitment(
        config.get_rpc_url(),
        Duration::from_secs(10),
        CommitmentConfig::confirmed(),
    ));

    let is_rpc_node_local = config.get_rpc_url().contains("127.0.0.1");

    info!("Starting indexer...");
    // For localnet we can safely use a large batch size to speed up indexing.
    let max_concurrent_block_fetches = match config.max_concurrent_block_fetches {
        Some(max_concurrent_block_fetches) => max_concurrent_block_fetches,
        None => {
            if is_rpc_node_local {
                200
            } else {
                20
            }
        }
    };

    let messenger = Arc::new(messenger::Messenger::new(config.clone()));
    messenger.clone().run(dao.clone());

    let mut last_indexed_slot = 0;
    if config.start_slot != 0 {
        last_indexed_slot = fetch_block_parent_slot(rpc_client.clone(), config.start_slot).await;
    }

    let block_stream_config = BlockStreamConfig {
        rpc_client: rpc_client.clone(),
        max_concurrent_block_fetches,
        last_indexed_slot,
        grpc_url: config.grpc_url.clone(),
        index_recent: config.index_recent.unwrap_or(true),
    };

    let streamer: Box<dyn Streamer + Send + Sync + 'static> = if config.grpc_url.is_some() {
        Box::new(GrpcStreamer::new(block_stream_config))
            as Box<dyn Streamer + Send + Sync + 'static>
    } else {
        Box::new(PollerStreamer::new(block_stream_config))
            as Box<dyn Streamer + Send + Sync + 'static>
    };

    let indexer_handle = tokio::task::spawn(continously_index_new_blocks(
        streamer,
        messenger,
        rpc_client.clone(),
        last_indexed_slot,
    ));

    match tokio::signal::ctrl_c().await {
        Ok(()) => {
            info!("Shutting down indexer...");
            indexer_handle.abort();

            // Wait for the task to finish, checking if it was indeed aborted.
            match indexer_handle.await {
                Ok(_) => {
                    error!("Indexer task completed unexpectedly");
                }
                Err(err) if err.is_cancelled() => {
                    info!("Indexer task was successfully aborted");
                }
                Err(err) => {
                    error!("Unexpected error while waiting for indexer task: {:?}", err);
                }
            }
        }
        Err(err) => {
            error!("Unable to listen for shutdown signal: {}", err);
        }
    }
    Ok(())
}
