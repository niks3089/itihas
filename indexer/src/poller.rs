use std::{sync::Arc, thread::sleep, time::Duration};

use async_stream::stream;
use futures::{pin_mut, stream::FuturesUnordered, Stream, StreamExt};
use log::info;
use solana_client::{
    nonblocking::rpc_client::RpcClient, rpc_config::RpcBlockConfig, rpc_request::RpcError,
};
use solana_sdk::commitment_config::CommitmentConfig;
use solana_transaction_status::{TransactionDetails, UiTransactionEncoding};

use crate::{
    error::IndexerError,
    messenger::Messenger,
    parser::parse_ui_confirmed_block,
    streamer::{fetch_current_slot, Streamer},
    types::{BlockInfo, BlockStreamConfig},
};

const POST_BACKFILL_FREQUENCY: u64 = 100;
const PRE_BACKFILL_FREQUENCY: u64 = 10;
const SKIPPED_BLOCK_ERRORS: [i64; 2] = [-32007, -32009];
const FAILED_BLOCK_LOGGING_FREQUENCY: u64 = 100;

pub struct PollerStreamer {
    config: BlockStreamConfig,
}

impl Streamer for PollerStreamer {
    fn load_block_stream(&self, slot: u64) -> impl Stream<Item = BlockInfo> {
        self.get_poller_block_stream(slot)
    }
}

impl PollerStreamer {
    pub fn new(config: BlockStreamConfig) -> Self {
        Self { config }
    }

    async fn get_block(client: &RpcClient, slot: u64) -> Result<BlockInfo, IndexerError> {
        let mut attempt_counter = 0;
        loop {
            match client
                .get_block_with_config(
                    slot,
                    RpcBlockConfig {
                        encoding: Some(UiTransactionEncoding::Base64),
                        transaction_details: Some(TransactionDetails::Full),
                        rewards: None,
                        commitment: Some(CommitmentConfig::confirmed()),
                        max_supported_transaction_version: Some(0),
                    },
                )
                .await
            {
                Ok(block) => match parse_ui_confirmed_block(block, slot) {
                    Ok(block_info) => return Ok(block_info),
                    Err(e) => return Err(e),
                },
                Err(e) => {
                    if let solana_client::client_error::ClientErrorKind::RpcError(
                        RpcError::RpcResponseError { code, .. },
                    ) = e.kind
                    {
                        if SKIPPED_BLOCK_ERRORS.contains(&code) {
                            log::warn!("Skipped block: {}", slot);
                            return Err(IndexerError::ParserError(e.to_string()));
                        }
                    }
                    if attempt_counter % FAILED_BLOCK_LOGGING_FREQUENCY == 1 {
                        log::warn!("Failed to fetch block: {}. {}", slot, e.to_string());
                    }
                    attempt_counter += 1;
                }
            }
        }
    }

    fn get_poller_block_stream(&self, latest_slot: u64) -> impl futures::Stream<Item = BlockInfo> {
        let client = self.config.rpc_client.clone();
        let last_indexed_slot = self.config.last_indexed_slot;
        let max_concurrent_block_fetches = self.config.max_concurrent_block_fetches;

        stream! {
            let mut current_slot_to_fetch = match last_indexed_slot {
                0 => 0,
                last_indexed_slot => last_indexed_slot + 1
            };

            if latest_slot != 0 {
                current_slot_to_fetch = current_slot_to_fetch.max(latest_slot);
            }
            let polls_forever = true;
            let mut end_block_slot = fetch_current_slot(client.as_ref()).await;

            loop {
                if current_slot_to_fetch > end_block_slot  && !polls_forever {
                    break;
                }

                while current_slot_to_fetch > end_block_slot {
                    end_block_slot = fetch_current_slot(client.as_ref()).await;
                    if end_block_slot <= current_slot_to_fetch {
                        sleep(Duration::from_millis(10));
                    }
                }

                let mut block_fetching_futures_batch = vec![];
                while block_fetching_futures_batch.len() < max_concurrent_block_fetches && current_slot_to_fetch <= end_block_slot  {
                    let client = client.clone();
                    block_fetching_futures_batch.push(PollerStreamer::fetch_block_with_using_arc(
                        client.clone(),
                        current_slot_to_fetch,
                    ));
                    current_slot_to_fetch += 1;
                }
                let blocks_to_yield = block_fetching_futures_batch
                    .into_iter()
                    .collect::<FuturesUnordered<_>>()
                    .collect::<Vec<_>>()
                    .await;
                let mut blocks_to_yield: Vec<_>  = blocks_to_yield.into_iter().flatten().collect();
                blocks_to_yield.sort_by_key(|block| block.metadata.slot);
                for block in blocks_to_yield.drain(..) {
                    yield block;
                }

            }
        }
    }

    async fn fetch_block_with_using_arc(
        client: Arc<RpcClient>,
        slot: u64,
    ) -> Result<BlockInfo, IndexerError> {
        Self::get_block(client.as_ref(), slot).await
    }
}

pub async fn continously_index_new_blocks(
    poller_fetcher: PollerStreamer,
    messenger: Arc<Messenger>,
    rpc_client: Arc<RpcClient>,
    mut last_indexed_slot_at_start: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let current_slot = fetch_current_slot(rpc_client.as_ref()).await;
        if last_indexed_slot_at_start == 0 {
            last_indexed_slot_at_start = current_slot;
        }
        let block_stream = poller_fetcher.load_block_stream(last_indexed_slot_at_start);
        pin_mut!(block_stream);

        let number_of_blocks_to_backfill = current_slot - last_indexed_slot_at_start;
        info!(
            "Backfilling historical blocks. Current number of blocks to backfill: {}, Current slot: {}",
            number_of_blocks_to_backfill, current_slot
        );
        let mut last_indexed_slot = last_indexed_slot_at_start;

        let mut finished_backfill = true;

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
                    info!("Finished backfilling historical blocks!");
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
