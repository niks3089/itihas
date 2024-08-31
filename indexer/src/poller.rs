use std::{pin::Pin, sync::Arc, thread::sleep, time::Duration};

use async_stream::stream;
use futures::{stream::FuturesUnordered, Stream, StreamExt};
use solana_client::{
    nonblocking::rpc_client::RpcClient, rpc_config::RpcBlockConfig, rpc_request::RpcError,
};
use solana_sdk::commitment_config::CommitmentConfig;
use solana_transaction_status::{TransactionDetails, UiTransactionEncoding};

use crate::{
    error::IndexerError,
    parser::PollerParser,
    streamer::{fetch_current_slot, Streamer},
    types::{BlockInfo, BlockStreamConfig},
};

const SKIPPED_BLOCK_ERRORS: [i64; 2] = [-32007, -32009];
const FAILED_BLOCK_LOGGING_FREQUENCY: u64 = 100;

#[derive(Clone)]
pub struct PollerStreamer {
    config: BlockStreamConfig,
}

impl Streamer for PollerStreamer {
    fn load_block_stream(&self, slot: u64) -> Pin<Box<dyn Stream<Item = BlockInfo> + Send + '_>> {
        Box::pin(PollerStreamer::get_poller_block_stream(
            self.config.rpc_client.clone(),
            self.config.last_indexed_slot,
            self.config.max_concurrent_block_fetches,
            Some(slot),
        ))
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
                Ok(block) => match PollerParser::parse_ui_confirmed_block(block, slot) {
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

    pub fn get_poller_block_stream(
        client: Arc<RpcClient>,
        last_indexed_slot: u64,
        max_concurrent_block_fetches: usize,
        end_block_slot: Option<u64>,
    ) -> impl futures::Stream<Item = BlockInfo> {
        stream! {
            let mut current_slot_to_fetch = match last_indexed_slot {
                0 => 0,
                last_indexed_slot => last_indexed_slot + 1
            };

            let polls_forever = end_block_slot.is_none();
            let mut end_block_slot = end_block_slot.unwrap_or(fetch_current_slot(client.as_ref()).await);
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
