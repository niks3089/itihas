use solana_sdk::signature::Signature;
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
use yellowstone_grpc_proto::geyser::{SubscribeUpdateBlock, SubscribeUpdateTransactionInfo};
use yellowstone_grpc_proto::prelude::{InnerInstructions, TransactionError};
use std::{fmt, str::FromStr};
use log::error;

use std::convert::TryFrom;

use crate::{
    error::IndexerError,
    types::{
        BlockInfo, BlockMetadata, Instruction, InstructionGroup, StateUpdate, Transaction,
    },
};

const SPL_ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID: &str =
    "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";

pub struct PollerParser {}

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
            instruction_groups: PollerParser::parse_instruction_groups(versioned_transaction, meta.clone())?,
            signature,
            error,
            slot: 0,
            block_time: 0,
        })
    }
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

impl PollerParser {
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
            .map(|tx| Self::parse_encoded_transaction(tx, slot, block_time))
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

    fn parse_encoded_transaction(
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
        let instruction_groups = Self::parse_instruction_groups(versioned_transaction, meta)?;

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
            if program_id_index >= accounts.len(){
                return Err(IndexerError::ParserError("Program ID index out of bounds".to_string()));
            }
            let program_id = accounts[program_id_index];
            let data = ix.data.clone();
            let instruction_accounts: Vec<Pubkey> = ix
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

            if (program_id == token_program_id || program_id == token_extensions_program_id)
                && instruction_accounts.len() >= 2
            {
                if let Ok(transfer_instruction) = spl_token::instruction::TokenInstruction::unpack(&data) {
                    if let spl_token::instruction::TokenInstruction::Transfer { amount } = transfer_instruction {
                        let source_address = instruction_accounts[0];
                        let destination_address = instruction_accounts[1];

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
                        let source_ata = find_associated_token_address(source_address, mint, Some(token_program_id))?;
                        let destination_ata = find_associated_token_address(destination_address, mint, Some(token_program_id))?;

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
                                                        let inner_source_address = inner_accounts[0].to_bytes().to_vec();
                                                        let inner_destination_address = inner_accounts[1].to_bytes().to_vec();
                                                        
                                                        inner_instructions.push(Instruction {
                                                            program_id: inner_program_id,
                                                            data: inner_data,
                                                            accounts: inner_accounts,
                                                            source_address: inner_source_address,
                                                            destination_address: inner_destination_address,
                                                            source_ata: None,
                                                            destination_ata: None,
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
                                accounts: accounts.clone(),
                                source_address: source_address.to_bytes().to_vec(),
                                destination_address: destination_address.to_bytes().to_vec(),
                                source_ata: Some(source_ata.to_bytes().to_vec()),
                                destination_ata: Some(destination_ata.to_bytes().to_vec()),
                                mint: Some(mint.to_bytes().to_vec()),
                                amount,
                            },
                            inner_instructions,
                        });
                    }
                }
            }
        }

        Ok(instruction_groups)
    }
}

pub struct GrpcParser{}

impl GrpcParser {

    fn transaction_error_to_string(error: &TransactionError) -> String {
        match std::str::from_utf8(&error.err) {
            Ok(s) => s.to_string(),
            Err(_) => "Invalid UTF-8 in TransactionError".to_string(),
        }
    }
    pub fn parse_transaction(
        transaction: SubscribeUpdateTransactionInfo,
        slot: u64,
        block_time: i64,
    ) -> Result<Option<Transaction>, IndexerError> {
        let meta = transaction
            .meta
            .ok_or(IndexerError::ParserError("Missing metadata".to_string()))?;

        let error = meta.clone().err.map(|e| Self::transaction_error_to_string(&e));

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
                let source_address = instruction_accounts[0];
                let destination_address = instruction_accounts[1];

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

                    let source_ata = Some(
                        find_associated_token_address(source_address, mint, Some(program_id))?
                            .to_bytes()
                            .to_vec(),
                    );
                    let destination_ata = Some(
                        find_associated_token_address(destination_address, mint, Some(program_id))?
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
                                source_address: source_address.to_bytes().to_vec(),
                                destination_address: destination_address.to_bytes().to_vec(),
                                source_ata: None,
                                destination_ata: None,
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
                            source_address: source_address.to_bytes().to_vec(),
                            destination_address: destination_address.to_bytes().to_vec(),
                            source_ata,
                            destination_ata,
                            mint: Some(mint.to_bytes().to_vec()),
                            amount,
                        },
                        inner_instructions,
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

    pub fn parse_block(block: SubscribeUpdateBlock) -> Result<BlockInfo, IndexerError> {
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
                Self::parse_transaction(transaction, metadata.slot, metadata.block_time)
            })
            .filter_map(|result| match result {
                Ok(Some(transaction)) => Some(Ok(transaction)),
                Ok(None) => None,
                Err(e) => Some(Err(e)),
            })
            .collect();

        let transactions = transactions?;
        Ok(BlockInfo {
            metadata,
            transactions,
        })
    }

}