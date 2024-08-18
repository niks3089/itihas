use std::{collections::HashSet, sync::Arc};

use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    clock::{Slot, UnixTimestamp},
    pubkey::Pubkey,
    signature::Signature,
};

// To avoid exceeding the 64k total parameter limit
pub const MAX_SQL_INSERTS: usize = 5000;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Instruction {
    pub program_id: Pubkey,
    pub data: Vec<u8>,
    pub accounts: Vec<Pubkey>,
    pub src_address: Vec<u8>,
    pub dest_address: Vec<u8>,
    pub mint: Option<Vec<u8>>,
    pub src_ata: Option<Vec<u8>>,
    pub dest_ata: Option<Vec<u8>>,
    pub amount: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InstructionGroup {
    pub outer_instruction: Instruction,
    pub inner_instructions: Vec<Instruction>,
    pub token_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Transaction {
    pub instruction_groups: Vec<InstructionGroup>,
    pub signature: Signature,
    pub block_time: UnixTimestamp,
    pub error: Option<String>,
    pub slot: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlockInfo {
    pub metadata: BlockMetadata,
    pub transactions: Vec<Transaction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlockMetadata {
    pub slot: Slot,
    pub parent_slot: Slot,
    pub block_time: UnixTimestamp,
    pub blockhash: String,
    pub parent_blockhash: String,
    pub block_height: u64,
}

#[derive(Clone)]
pub struct BlockStreamConfig {
    pub rpc_client: Arc<RpcClient>,
    pub grpc_url: Option<String>,
    pub max_concurrent_block_fetches: usize,
    pub last_indexed_slot: u64,
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct StateUpdate {
    pub transactions: HashSet<Transaction>,
}

impl StateUpdate {
    pub fn new() -> Self {
        StateUpdate::default()
    }

    pub fn merge_updates(updates: Vec<StateUpdate>) -> StateUpdate {
        let mut merged = StateUpdate::default();
        for update in updates {
            merged.transactions.extend(update.transactions);
        }
        merged
    }
}
