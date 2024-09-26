use std::{collections::HashMap, pin::Pin, time::Duration};

use async_std::stream::StreamExt;
use async_stream::stream;
use cadence_macros::statsd_count;
use common::metric;
use futures::{
    future::{select, Either},
    pin_mut, SinkExt, Stream,
};
use log::{error, info};
use rand::distributions::Alphanumeric;
use rand::Rng;
use tokio::time::sleep;
use yellowstone_grpc_client::{GeyserGrpcBuilderResult, GeyserGrpcClient, Interceptor};
use yellowstone_grpc_proto::geyser::{
    subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest, SubscribeRequestFilterBlocks,
    SubscribeRequestPing,
};

use crate::{
    parser::GrpcParser,
    poller::PollerStreamer,
    streamer::Streamer,
    types::{BlockInfo, BlockStreamConfig},
};

pub struct GrpcStreamer {
    config: BlockStreamConfig,
}
impl Streamer for GrpcStreamer {
    fn load_block_stream(&self, slot: u64) -> Pin<Box<dyn Stream<Item = BlockInfo> + Send + '_>> {
        Box::pin(self.get_grpc_stream_with_rpc_fallback(slot))
    }
}

impl GrpcStreamer {
    pub fn new(config: BlockStreamConfig) -> Self {
        Self { config }
    }

    pub fn get_grpc_stream_with_rpc_fallback(
        &self,
        latest_slot: u64,
    ) -> impl Stream<Item = BlockInfo> + '_ {
        let rpc_client = self.config.rpc_client.clone();
        let mut last_indexed_slot = self.config.last_indexed_slot;
        let max_concurrent_block_fetches = self.config.max_concurrent_block_fetches;
        let endpoint = self.config.grpc_url.clone().unwrap();
        let auth_header = self.config.grpc_x_token.clone();
        stream! {
            let grpc_stream = self.get_grpc_block_stream(endpoint, auth_header);
            pin_mut!(grpc_stream);
            let mut rpc_poll_stream:  Option<Pin<Box<dyn Stream<Item = BlockInfo> + Send>>> = None;
            // Await either the gRPC stream or the RPC block fetching
            loop {
                match rpc_poll_stream.as_mut() {
                    Some(rpc_poll_stream_value) => {
                        match select(grpc_stream.next(), rpc_poll_stream_value.next()).await {
                            Either::Left((Some(grpc_block), _)) => {

                                if grpc_block.metadata.parent_slot == last_indexed_slot || self.config.index_recent {
                                    last_indexed_slot = grpc_block.metadata.slot;
                                    yield grpc_block;
                                    rpc_poll_stream = None;
                                }

                            }
                            Either::Left((None, _)) => {
                                panic!("gRPC stream ended unexpectedly");
                            }
                            Either::Right((Some(rpc_block), _)) => {
                                if rpc_block.metadata.parent_slot == last_indexed_slot {
                                    last_indexed_slot = rpc_block.metadata.slot;
                                    yield rpc_block;
                                }
                            }
                            Either::Right((None, _)) => {
                                rpc_poll_stream = None;
                                info!("Switching back to gRPC block fetching");
                            }
                        }
                    }
                    None => {
                        let block = grpc_stream.next().await.unwrap();
                        if block.metadata.slot == 0 {
                            continue;
                        }
                        if block.metadata.parent_slot == last_indexed_slot || self.config.index_recent {
                            last_indexed_slot = block.metadata.slot;
                            yield block;
                        } else {
                            info!("Switching to RPC block fetching");
                            rpc_poll_stream = Some(Box::pin(PollerStreamer::get_poller_block_stream(
                                rpc_client.clone(),
                                last_indexed_slot,
                                max_concurrent_block_fetches,
                                Some(block.metadata.slot),
                            )));
                        }

                    }
                }


            }
        }
    }

    fn get_grpc_block_stream(
        &self,
        endpoint: String,
        auth_header: String,
    ) -> impl Stream<Item = BlockInfo> + '_ {
        stream! {
            loop {
                let mut grpc_tx;
                let mut grpc_rx;
                {
                    yield BlockInfo::default();
                    let grpc_client =
                        self.build_geyser_client(endpoint.clone(), auth_header.clone()).await;
                    if let Err(e) = grpc_client {
                        error!("Error connecting to gRPC, waiting one second then retrying connect: {}", e);
                        metric! {
                            statsd_count!("grpc_connect_error", 1);
                        }

                        sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                    let subscription = grpc_client
                        .unwrap()
                        .subscribe_with_request(Some(self.get_block_subscribe_request()))
                        .await;
                    if let Err(e) = subscription {
                        error!("Error subscribing to gRPC stream, waiting one second then retrying connect: {}", e);
                        metric! {
                            statsd_count!("grpc_subscribe_error", 1);
                        }
                        sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                    (grpc_tx, grpc_rx) = subscription.unwrap();
                }
                while let Some(message) = grpc_rx.next().await {
                    match message {
                        Ok(message) => match message.update_oneof {
                            Some(UpdateOneof::Block(block)) => {
                                match GrpcParser::parse_block(block) {
                                    Ok(parsed_block) => {
                                        yield parsed_block
                                    }
                                    Err(error) => {
                                        error!("Error parsing block: {:?}", error);
                                        metric! {
                                            statsd_count!("grpc_parsing_block_error", 1);
                                        }
                                        continue;
                                    }
                                }
                            }
                            Some(UpdateOneof::Ping(_)) => {
                                // This is necessary to keep load balancers that expect client pings alive. If your load balancer doesn't
                                // require periodic client pings then this is unnecessary
                                let ping = grpc_tx.send(self.ping()).await;
                                if let Err(e) = ping {
                                    error!("Error sending ping: {}", e);
                                    metric! {
                                        statsd_count!("grpc_ping_error", 1);
                                    }
                                    break;
                                }
                            }
                            Some(UpdateOneof::Pong(_)) => {}
                            _ => {
                                error!("Unknown message: {:?}", message);
                            }
                        },
                        Err(error) => {
                            error!(
                                "error in block subscribe, resubscribing in 1 second: {error:?}"
                            );
                            metric! {
                                statsd_count!("grpc_resubscribe", 1);
                            }
                            break;
                        }
                    }
                }
            sleep(Duration::from_secs(1)).await;
            }
        }
    }

    async fn build_geyser_client(
        &self,
        endpoint: String,
        auth_header: String,
    ) -> GeyserGrpcBuilderResult<GeyserGrpcClient<impl Interceptor>> {
        GeyserGrpcClient::build_from_shared(endpoint)?
            .x_token(Some(auth_header))?
            .connect_timeout(Duration::from_secs(10))
            .max_decoding_message_size(8388608)
            .timeout(Duration::from_secs(10))
            .connect()
            .await
    }

    fn ping(&self) -> SubscribeRequest {
        SubscribeRequest {
            ping: Some(SubscribeRequestPing { id: 1 }),
            ..Default::default()
        }
    }

    fn get_block_subscribe_request(&self) -> SubscribeRequest {
        SubscribeRequest {
            blocks: HashMap::from_iter(vec![(
                self.generate_random_string(20),
                SubscribeRequestFilterBlocks {
                    account_include: vec![],
                    include_transactions: Some(true),
                    include_accounts: Some(false),
                    include_entries: Some(false),
                },
            )]),
            commitment: Some(CommitmentLevel::Confirmed.into()),
            ..Default::default()
        }
    }

    fn generate_random_string(&self, len: usize) -> String {
        rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(len)
            .map(char::from)
            .collect()
    }
}
