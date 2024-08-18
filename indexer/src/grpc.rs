use std::{collections::HashMap, pin::Pin, str::FromStr, time::Duration};

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
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use tokio::time::sleep;
use yellowstone_grpc_client::{GeyserGrpcBuilderResult, GeyserGrpcClient, Interceptor};
use yellowstone_grpc_proto::{
    geyser::{
        subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest,
        SubscribeRequestFilterBlocks, SubscribeRequestPing, SubscribeUpdateBlock,
        SubscribeUpdateTransactionInfo,
    },
    prelude::{InnerInstructions, TransactionError},
};

use crate::{
    error::IndexerError,
    parser::find_associated_token_address,
    poller::PollerStreamer,
    streamer::Streamer,
    types::{
        BlockInfo, BlockMetadata, BlockStreamConfig, Instruction, InstructionGroup, Transaction,
    },
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

        stream! {
            let grpc_stream = self.get_grpc_block_stream(endpoint, None);
            pin_mut!(grpc_stream);
            let mut rpc_poll_stream:  Option<Pin<Box<dyn Stream<Item = BlockInfo> + Send>>> = None;
            // Await either the gRPC stream or the RPC block fetching
            loop {
                match rpc_poll_stream.as_mut() {
                    Some(rpc_poll_stream_value) => {
                        match select(grpc_stream.next(), rpc_poll_stream_value.next()).await {
                            Either::Left((Some(grpc_block), _)) => {

                                if grpc_block.metadata.parent_slot == last_indexed_slot {
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
                        if block.metadata.parent_slot == last_indexed_slot {
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
        auth_header: Option<String>,
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
                        statsd_count!("grpc_connect_error", 1);

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
                                match self.parse_block(block) {
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
        auth_header: Option<String>,
    ) -> GeyserGrpcBuilderResult<GeyserGrpcClient<impl Interceptor>> {
        GeyserGrpcClient::build_from_shared(endpoint)?
            .x_token(auth_header)?
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

    fn parse_transaction(
        &self,
        transaction: SubscribeUpdateTransactionInfo,
        slot: u64,
        block_time: i64,
    ) -> Result<Option<Transaction>, IndexerError> {
        let meta = transaction
            .meta
            .ok_or(IndexerError::ParserError("Missing metadata".to_string()))?;

        let error = meta.clone().err.map(|e| transaction_error_to_string(&e));

        let signature = Signature::try_from(transaction.signature)
            .map_err(|_| IndexerError::ParserError("error parsing signature".to_string()))?;
        let message = transaction
            .transaction
            .ok_or(IndexerError::ParserError("Missing transaction".to_string()))?
            .message
            .ok_or(IndexerError::ParserError("Missing message".to_string()))?;

        let mut accounts = message.account_keys;
        for account in meta.loaded_writable_addresses {
            accounts.push(account);
        }
        for account in meta.loaded_readonly_addresses {
            accounts.push(account);
        }

        let mut instruction_groups: Vec<InstructionGroup> = Vec::new();

        for ix in message.instructions.iter() {
            let program_id_index = ix.program_id_index as usize;
            if program_id_index >= accounts.len() {
                return Err(IndexerError::ParserError(
                    "Program ID index out of bounds".to_string(),
                ));
            }
            let program_id = Pubkey::try_from(accounts[program_id_index].clone())
                .map_err(|_| IndexerError::ParserError("error parsing program id".to_string()))?;
            let data = ix.data.clone();
            let instruction_accounts: Vec<Pubkey> = ix
                .accounts
                .iter()
                .map(|account_index| {
                    let account_index = *account_index as usize;
                    if account_index >= accounts.len() {
                        return Err(IndexerError::ParserError(
                            "Account index out of bounds".to_string(),
                        ));
                    }
                    Pubkey::try_from(accounts[account_index].clone()).map_err(|_| {
                        IndexerError::ParserError("error getting accounts from grpc".to_string())
                    })
                })
                .collect::<Result<Vec<_>, IndexerError>>()?;

            let token_program_id = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")?;
            let token_extensions_program_id =
                Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")?;
            let mut inner_instructions = Vec::new();

            if (program_id == token_program_id || program_id == token_extensions_program_id)
                && instruction_accounts.len() >= 2
            {
                let src_address = instruction_accounts[0].to_bytes().to_vec();
                let dest_address = instruction_accounts[1].to_bytes().to_vec();

                if let Ok(spl_token::instruction::TokenInstruction::Transfer { amount }) =
                    spl_token::instruction::TokenInstruction::unpack(&data)
                {
                    let mint = meta
                        .post_token_balances
                        .first()
                        .map(|balance| Pubkey::from_str(&balance.mint))
                        .transpose()?
                        .ok_or(IndexerError::ParserError(
                            "Token balance not found".to_string(),
                        ))?;

                    let src_ata = Some(
                        find_associated_token_address(
                            instruction_accounts[0],
                            mint,
                            Some(program_id),
                        )?
                        .to_bytes()
                        .to_vec(),
                    );
                    let dest_ata = Some(
                        find_associated_token_address(
                            instruction_accounts[1],
                            mint,
                            Some(program_id),
                        )?
                        .to_bytes()
                        .to_vec(),
                    );
                    for inner_instruction_group in meta.inner_instructions.iter() {
                        let InnerInstructions {
                            index: _,
                            instructions,
                        } = inner_instruction_group;
                        for instruction in instructions {
                            let inner_data = instruction.data.clone();
                            let inner_accounts: Vec<Pubkey> = instruction
                                .accounts
                                .iter()
                                .filter_map(|account_index| {
                                    let account_index = *account_index as usize;
                                    if account_index >= accounts.len() {
                                        error!(
                                            "Error: Account index out of bounds: {} (len: {}). Skipping this account.",
                                            account_index, accounts.len()
                                        );
                                        return None;
                                    }
                                    let pubkey =
                                        Pubkey::try_from(accounts[account_index].clone()).ok()?;
                                    Some(pubkey)
                                })
                                .collect();

                            inner_instructions.push(Instruction {
                                program_id,
                                data: inner_data,
                                accounts: inner_accounts,
                                src_address: src_address.clone(),
                                dest_address: dest_address.clone(),
                                src_ata: None,
                                dest_ata: None,
                                mint: None,
                                amount,
                            });
                        }
                    }

                    instruction_groups.push(InstructionGroup {
                        outer_instruction: Instruction {
                            program_id,
                            data,
                            accounts: instruction_accounts,
                            src_address,
                            dest_address,
                            src_ata,
                            dest_ata,
                            mint: Some(mint.to_bytes().to_vec()),
                            amount,
                        },
                        inner_instructions,
                        token_type: "spl_token".to_string(),
                    });
                }
            }
        }
        if instruction_groups.is_empty() {
            return Ok(None);
        }

        Ok(Some(Transaction {
            instruction_groups,
            signature,
            error,
            slot,
            block_time,
        }))
    }

    fn parse_block(&self, block: SubscribeUpdateBlock) -> Result<BlockInfo, IndexerError> {
        let metadata = BlockMetadata {
            slot: block.slot,
            parent_slot: block.parent_slot,
            block_time: block.block_time.unwrap().timestamp,
            blockhash: block.blockhash,
            parent_blockhash: block.parent_blockhash,
            block_height: block.block_height.unwrap().block_height,
        };

        let transactions: Result<Vec<Transaction>, IndexerError> = block
            .transactions
            .into_iter()
            .map(|transaction| {
                self.parse_transaction(transaction, metadata.slot, metadata.block_time)
            })
            .filter_map(|result| match result {
                Ok(Some(transaction)) => Some(Ok(transaction)),
                Ok(None) => None,       // Filter out None values
                Err(e) => Some(Err(e)), // Propagate the error
            })
            .collect();

        let transactions = transactions?;
        Ok(BlockInfo {
            metadata,
            transactions,
        })
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

fn transaction_error_to_string(error: &TransactionError) -> String {
    match std::str::from_utf8(&error.err) {
        Ok(s) => s.to_string(),
        Err(_) => "Invalid UTF-8 in TransactionError".to_string(),
    }
}
