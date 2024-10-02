use std::{pin::Pin, sync::Arc, thread::sleep, time::Duration};

use cadence_macros::statsd_count;
use common::metric;
use futures::{pin_mut, Stream};
use log::{error, info, warn};
use solana_client::{nonblocking::rpc_client::RpcClient, rpc_config::RpcBlockConfig};
use solana_sdk::commitment_config::CommitmentConfig;
use solana_transaction_status::{TransactionDetails, UiTransactionEncoding};
use tokio_stream::StreamExt;

use crate::{messenger::Messenger, types::BlockInfo};

const POST_BACKFILL_FREQUENCY: u64 = 100;
const PRE_BACKFILL_FREQUENCY: u64 = 10;

pub trait Streamer: Send {
    fn load_block_stream(&self, slot: u64) -> Pin<Box<dyn Stream<Item = BlockInfo> + Send + '_>>;
}

pub async fn get_genesis_hash(rpc_client: &RpcClient) -> String {
    loop {
        match rpc_client.get_genesis_hash().await {
            Ok(genesis_hash) => return genesis_hash.to_string(),
            Err(e) => {
                error!("Failed to fetch genesis hash: {}", e);
                metric! {
                    statsd_count!("get_genesis_hash_error", 1);
                }
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
                error!("Failed to fetch current slot: {}", e);
                sleep(Duration::from_secs(5));
            }
        }
    }
}

pub async fn continously_index_new_blocks(
    streamer: Box<dyn Streamer + Send + Sync>,
    messenger: Arc<Messenger>,
    rpc_client: Arc<RpcClient>,
    mut last_indexed_slot_at_start: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let current_slot = fetch_current_slot(rpc_client.as_ref()).await;
        if last_indexed_slot_at_start == 0 {
            last_indexed_slot_at_start = current_slot;
        }
        let block_stream = streamer.load_block_stream(last_indexed_slot_at_start);
        pin_mut!(block_stream);

        let number_of_blocks_to_backfill = current_slot - last_indexed_slot_at_start;

        let mut last_indexed_slot = last_indexed_slot_at_start;

        // Temp hack to not backfill or backfill blocks when we restart the indexer
        let mut finished_backfill = false;
        if !finished_backfill {
            warn!(
                "Backfilling historical blocks. Current number of blocks to backfill: {}, Current slot: {}",
                number_of_blocks_to_backfill, current_slot
            );
        }

        loop {
            let block = block_stream.next().await.unwrap();
            let slot_indexed = block.metadata.slot;
            messenger.send_block_batches(vec![block]).await;

            if !finished_backfill {
                let blocks_indexed = slot_indexed - last_indexed_slot_at_start;
                if blocks_indexed <= number_of_blocks_to_backfill {
                    if blocks_indexed % PRE_BACKFILL_FREQUENCY == 0 {
                        info!(
                            "Backfilled {} / {} blocks",
                            blocks_indexed, number_of_blocks_to_backfill
                        );
                    }
                } else {
                    finished_backfill = true;
                    warn!("Finished backfilling historical blocks!");
                }
            } else {
                for slot in last_indexed_slot..slot_indexed {
                    if slot % POST_BACKFILL_FREQUENCY == 0 {
                        info!("Indexed slot {}", slot);
                    }
                }
            }

            last_indexed_slot = slot_indexed;
        }
    })
}
