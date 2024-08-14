use solana_sdk::{
    bs58,
    clock::Slot,
    pubkey::Pubkey,
    transaction::VersionedTransaction,
};
use solana_transaction_status::{
    option_serializer::OptionSerializer, EncodedConfirmedTransactionWithStatusMeta,
    EncodedTransactionWithStatusMeta, UiConfirmedBlock, UiInstruction, UiTransactionStatusMeta,
};
use std::{fmt, str::FromStr};

use std::convert::TryFrom;

use crate::{
    error::IndexerError,
    types::{
        BlockInfo, BlockMetadata, Instruction, InstructionGroup, StateUpdate, Transaction,
    },
};

const SPL_ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID: &str =
    "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";


pub fn find_associated_token_address(
    owner: Pubkey,
    mint: Pubkey,
    program_id: Option<Pubkey>,
) -> Result<Pubkey, IndexerError> {
    let associated_token_program_id = Pubkey::from_str(SPL_ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID)
        .map_err(|_| IndexerError::ParserError(SPL_ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID.to_owned()))?;

    let token_program_id = program_id.ok_or(IndexerError::ParserError("invalid program id".to_owned()))?;

    Ok(Pubkey::find_program_address(
        &[owner.as_ref(), token_program_id.as_ref(), mint.as_ref()],
        &associated_token_program_id,
    )
    .0)
}

pub fn parse_block_state_update(block: &BlockInfo) -> Result<StateUpdate, IndexerError> {
    let mut state_updates: Vec<StateUpdate> = Vec::new();
    for transaction in &block.transactions {
        state_updates.push(parse_transaction(transaction)?);
    }
    Ok(StateUpdate::merge_updates(state_updates))
}

pub fn parse_transaction(
    tx: &Transaction,
) -> Result<StateUpdate, IndexerError> {
    let state_updates = Vec::new();

    let mut state_update = StateUpdate::merge_updates(state_updates);
    state_update.transactions.insert(tx.clone());
    Ok(state_update)
}

pub fn parse_ui_confirmed_block(
    block: UiConfirmedBlock,
    slot: Slot,
) -> Result<BlockInfo, IndexerError> {
    let UiConfirmedBlock {
        parent_slot,
        block_time,
        transactions,
        blockhash,
        previous_blockhash,
        block_height,
        ..
    } = block;

    let block_time = block_time
    .ok_or(IndexerError::ParserError("Missing block_time".to_string()))?;

    let transactions: Result<Vec<_>, _> = transactions
        .unwrap_or(Vec::new())
        .into_iter()
        .map(|tx| _parse_transaction(tx, slot, block_time))
        .collect();

    let transactions = transactions?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    Ok(BlockInfo {
        transactions,
        metadata: BlockMetadata {
            parent_slot,
            block_time,
            slot,
            block_height: block_height.ok_or(IndexerError::ParserError(
                "Missing block_height".to_string(),
            ))?,
            blockhash,
            parent_blockhash: previous_blockhash,
        },
    })
}

fn _parse_transaction(
    transaction: EncodedTransactionWithStatusMeta,
    slot: u64,
    block_time: i64,
) -> Result<Option<Transaction>, IndexerError> {
    let EncodedTransactionWithStatusMeta {
        transaction, meta, ..
    } = transaction;

    let versioned_transaction: VersionedTransaction = transaction.decode().ok_or(
        IndexerError::ParserError("Transaction cannot be decoded".to_string()),
    )?;
    let meta = meta.ok_or(IndexerError::ParserError("Missing metadata".to_string()))?;

    let signature = versioned_transaction.signatures[0];
    let error = meta.clone().err.map(|e| e.to_string());
    let instruction_groups = parse_instruction_groups(versioned_transaction, meta)?;

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

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Instruction {{ program_id: {}}}", self.program_id,)
    }
}

impl fmt::Display for InstructionGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "InstructionGroup {{ outer_instruction: {}, inner_instructions: [{}] }}",
            self.outer_instruction,
            self.inner_instructions
                .iter()
                .map(Instruction::to_string)
                .collect::<Vec<_>>()
                .join(", "),
        )
    }
}

impl fmt::Display for Transaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Transaction {{ instruction_groups: [{}] }}",
            self.instruction_groups
                .iter()
                .map(InstructionGroup::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl TryFrom<EncodedConfirmedTransactionWithStatusMeta> for Transaction {
    type Error = IndexerError;

    fn try_from(tx: EncodedConfirmedTransactionWithStatusMeta) -> Result<Self, Self::Error> {
        let EncodedConfirmedTransactionWithStatusMeta { transaction, .. } = tx;

        let EncodedTransactionWithStatusMeta {
            transaction, meta, ..
        } = transaction;

        let versioned_transaction: VersionedTransaction = transaction.decode().ok_or(
            IndexerError::ParserError("Transaction cannot be decoded".to_string()),
        )?;
        let signature = versioned_transaction.signatures[0];
        let meta = meta.ok_or(IndexerError::ParserError("Missing metadata".to_string()))?;
        let error = meta.clone().err.map(|e| e.to_string());
        Ok(Transaction {
            instruction_groups: parse_instruction_groups(versioned_transaction, meta.clone())?,
            signature,
            error,
            slot: 0,
            block_time: 0,
        })
    }
}

#[allow(clippy::collapsible_match)]
pub fn parse_instruction_groups(
    versioned_transaction: VersionedTransaction,
    meta: UiTransactionStatusMeta,
) -> Result<Vec<InstructionGroup>, IndexerError> {
    let mut accounts = Vec::from(versioned_transaction.message.static_account_keys());
    if versioned_transaction
        .message
        .address_table_lookups()
        .is_some()
    {
        if let OptionSerializer::Some(loaded_addresses) = meta.loaded_addresses.clone() {
            for address in loaded_addresses
                .writable
                .iter()
                .chain(loaded_addresses.readonly.iter())
            {
                let pubkey = Pubkey::from_str(address)
                    .map_err(|e| IndexerError::ParserError(e.to_string()))?;
                accounts.push(pubkey);
            }
        }
    }

    let token_program_id = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")?;
    let token_extensions_program_id =
        Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")?;

    let mut instruction_groups: Vec<InstructionGroup> = Vec::new();

    for ix in versioned_transaction.message.instructions().iter() {
        let program_id_index = ix.program_id_index as usize;
        if program_id_index >= accounts.len() {
            return Err(IndexerError::ParserError("Program ID index out of bounds".to_string()));
        }
        let program_id = accounts[program_id_index];
        let data = ix.data.clone();
        let accounts: Vec<Pubkey> = ix
            .accounts
            .iter()
            .map(|account_index| {
                let account_index = *account_index as usize;
                if account_index >= accounts.len() {
                    return Err(IndexerError::ParserError("Account index out of bounds".to_string()));
                }
                Ok(accounts[account_index])
            })
            .collect::<Result<Vec<_>, IndexerError>>()?;

        // Check if this is a transfer instruction with src and dest accounts
        if (program_id == token_program_id || program_id == token_extensions_program_id)
            && accounts.len() >= 2
        {
            if let Ok(transfer_instruction) = spl_token::instruction::TokenInstruction::unpack(&data) {
                if let spl_token::instruction::TokenInstruction::Transfer { amount } = transfer_instruction {
                    let src_address = accounts[0];
                    let dest_address = accounts[1];//.to_bytes().to_vec();
                    //let authority = accounts[2];

                    let mint= match &meta.post_token_balances {
                        OptionSerializer::Some(balances) => {
                            let balance_info = balances.first().ok_or(IndexerError::ParserError("Token balance not found".to_string()))?;
                            Pubkey::from_str(&balance_info.mint)
                                .map_err(|e| IndexerError::ParserError(e.to_string()))?
                        },
                        OptionSerializer::None => {
                            return Err(IndexerError::ParserError("Post token balances are missing".to_string()));
                        },
                        OptionSerializer::Skip => {
                            return Err(IndexerError::ParserError("Post token balances were skipped".to_string()));
                        },
                    };
                    let src_ata = find_associated_token_address(src_address, mint, Some(token_program_id))?;
                    let dest_ata = find_associated_token_address(dest_address, mint, Some(token_program_id))?;

                    let mut inner_instructions = Vec::new();

                    if let OptionSerializer::Some(inner_instructions_vec) = meta.inner_instructions.as_ref() {
                        for inner_instructions_item in inner_instructions_vec.iter() {
                            let _index = inner_instructions_item.index;
                            for ui_instruction in inner_instructions_item.instructions.iter() {
                                match ui_instruction {
                                    UiInstruction::Compiled(ui_compiled_instruction) => {
                                        let inner_program_id_index = ui_compiled_instruction.program_id_index as usize;
                                        if inner_program_id_index >= accounts.len() {
                                            return Err(IndexerError::ParserError("Inner program ID index out of bounds".to_string()));
                                        }
                                        let inner_program_id = accounts[inner_program_id_index];
                                        let inner_data = bs58::decode(&ui_compiled_instruction.data)
                                            .into_vec()
                                            .map_err(|e| IndexerError::ParserError(e.to_string()))?;
                                        let inner_accounts: Vec<Pubkey> = ui_compiled_instruction
                                            .accounts
                                            .iter()
                                            .map(|account_index| {
                                                let account_index = *account_index as usize;
                                                if account_index >= accounts.len() {
                                                    return Err(IndexerError::ParserError("Inner account index out of bounds".to_string()));
                                                }
                                                Ok(accounts[account_index])
                                            })
                                            .collect::<Result<Vec<_>, IndexerError>>()?;

                                        if inner_program_id == token_program_id
                                            || inner_program_id == token_extensions_program_id
                                        {
                                            if let Ok(inner_transfer_instruction) = 
                                                spl_token::instruction::TokenInstruction::unpack(&inner_data) 
                                            {
                                                if let spl_token::instruction::TokenInstruction::Transfer { amount } = inner_transfer_instruction {
                                                    let inner_src_address = inner_accounts[0].to_bytes().to_vec();
                                                    let inner_dest_address = inner_accounts[1].to_bytes().to_vec();
                                                    
                                                    inner_instructions.push(Instruction {
                                                        program_id: inner_program_id,
                                                        data: inner_data,
                                                        accounts: inner_accounts,
                                                        src_address: inner_src_address,
                                                        dest_address: inner_dest_address,
                                                        src_ata: None,
                                                        dest_ata: None,
                                                        mint: None,
                                                        amount,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                    UiInstruction::Parsed(_) => {
                                        return Err(IndexerError::ParserError(
                                            "Parsed instructions are not implemented yet".to_string(),
                                        ));
                                    }
                                }
                            }
                        }
                    }

                    instruction_groups.push(InstructionGroup {
                        outer_instruction: Instruction {
                            program_id,
                            data,
                            accounts,
                            src_address: src_address.to_bytes().to_vec(),
                            dest_address: dest_address.to_bytes().to_vec(),
                            src_ata: Some(src_ata.to_bytes().to_vec()),
                            dest_ata: Some(dest_ata.to_bytes().to_vec()),
                            mint: Some(mint.to_bytes().to_vec()),
                            amount,
                        },
                        inner_instructions,
                        token_type: "spl_token".to_string(),
                    });
                }
            }
        }
    }

    Ok(instruction_groups)
}
