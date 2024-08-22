use std::{sync::Arc, thread::sleep, time::Duration};

use sea_orm::{sea_query::Expr, DatabaseConnection, FromQueryResult, TransactionTrait};

use chrono::{DateTime, NaiveDateTime, Utc};
use dao::generated::{blocks, token_transfers};
use log::debug;
use sea_orm::{
    sea_query::OnConflict, ConnectionTrait, DatabaseTransaction, EntityTrait, QuerySelect,
    QueryTrait, Set,
};

use crate::{
    error::IndexerError,
    parser::parse_block_state_update,
    types::{BlockInfo, BlockMetadata, StateUpdate, Transaction, MAX_SQL_INSERTS},
};

#[derive(FromQueryResult)]
pub struct SlotModel {
    // Postgres do not support u64 as return type. We need to use i64 and cast it to u64.
    pub slot: Option<i64>,
}

#[derive(Clone)]
pub struct Dao {
    pub db: Arc<DatabaseConnection>,
}

impl Dao {
    pub fn new(db: DatabaseConnection) -> Self {
        Dao { db: Arc::new(db) }
    }

    pub async fn index_block(&self, block: &BlockInfo) -> Result<(), IndexerError> {
        let txn = self.db.begin().await?;
        self.index_block_metadatas_without_commit(&txn, vec![&block.metadata])
            .await?;
        self.index_transaction_update(&txn, parse_block_state_update(block)?)
            .await?;
        txn.commit().await?;
        Ok(())
    }

    pub async fn index_block_batches(&self, block_batch: Vec<BlockInfo>) {
        loop {
            match self.index_block_batch(&block_batch).await {
                Ok(()) => return,
                Err(e) => {
                    let start_block = block_batch.first().unwrap().metadata.slot;
                    let end_block = block_batch.last().unwrap().metadata.slot;
                    log::error!(
                        "Failed to index block batch {}-{}. Got error {}",
                        start_block,
                        end_block,
                        e
                    );
                    sleep(Duration::from_secs(1));
                }
            }
        }
    }

    pub async fn index_block_batch(&self, block_batch: &[BlockInfo]) -> Result<(), IndexerError> {
        let tx = self.db.begin().await?;
        let block_metadatas: Vec<&BlockMetadata> =
            block_batch.iter().map(|b| &b.metadata).collect();
        self.index_block_metadatas_without_commit(&tx, block_metadatas)
            .await?;
        let mut state_updates = Vec::new();
        for block in block_batch {
            state_updates.push(parse_block_state_update(block)?);
        }
        self.index_transaction_update(&tx, StateUpdate::merge_updates(state_updates))
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn index_block_metadatas(
        &self,
        blocks: Vec<&BlockMetadata>,
    ) -> Result<(), IndexerError> {
        let tx = self.db.begin().await?;
        self.index_block_metadatas_without_commit(&tx, blocks)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn index_block_metadatas_without_commit(
        &self,
        tx: &DatabaseTransaction,
        blocks: Vec<&BlockMetadata>,
    ) -> Result<(), IndexerError> {
        for block_chunk in blocks.chunks(MAX_SQL_INSERTS) {
            let block_models: Vec<blocks::ActiveModel> = block_chunk
                .iter()
                .map(|block| {
                    Ok::<blocks::ActiveModel, IndexerError>(blocks::ActiveModel {
                        slot: Set(block.slot as i64),
                        parent_slot: Set(block.parent_slot as i64),
                        block_time: Set(block.block_time),
                        block_height: Set(block.block_height as i64),
                    })
                })
                .collect::<Result<Vec<blocks::ActiveModel>, IndexerError>>()?;

            let query = blocks::Entity::insert_many(block_models)
                .on_conflict(
                    OnConflict::columns([blocks::Column::Slot, blocks::Column::BlockTime])
                        .do_nothing()
                        .to_owned(),
                )
                .build(tx.get_database_backend());
            tx.execute(query).await?;
        }

        Ok(())
    }

    pub async fn index_transaction(
        &self,
        transactions: &[Transaction],
    ) -> Result<(), IndexerError> {
        let txn = self.db.begin().await?;
        self.index_transactions_without_commit(&txn, transactions)
            .await?;
        txn.commit().await?;
        Ok(())
    }

    pub async fn index_transactions_without_commit(
        &self,
        txn: &DatabaseTransaction,
        transactions: &[Transaction],
    ) -> Result<(), IndexerError> {
        let transaction_models = transactions
            .iter()
            .flat_map(|transaction| {
                transaction
                    .instruction_groups
                    .iter()
                    .map(move |instruction_group| {
                        let naive_datetime =
                            NaiveDateTime::from_timestamp(transaction.block_time, 0);
                        let datetime_utc: DateTime<Utc> =
                            DateTime::from_naive_utc_and_offset(naive_datetime, Utc);

                        token_transfers::ActiveModel {
                            signature: Set(Into::<[u8; 64]>::into(transaction.signature).to_vec()),
                            slot: Set(transaction.slot as i64),
                            error: Set(transaction.error.clone()),
                            block_time: Set(datetime_utc.into()),
                            created_at: Set(chrono::Utc::now().naive_utc()),
                            source_address: Set(instruction_group
                                .outer_instruction
                                .source_address
                                .clone()),
                            destination_address: Set(instruction_group
                                .outer_instruction
                                .destination_address
                                .clone()),
                            mint_address: Set(instruction_group.outer_instruction.mint.clone()),
                            source_ata: Set(instruction_group.outer_instruction.source_ata.clone()),
                            destination_ata: Set(instruction_group
                                .outer_instruction
                                .destination_ata
                                .clone()),
                            amount: Set(instruction_group.outer_instruction.amount as i64),
                            token_type: Set(instruction_group.token_type.clone()),
                        }
                    })
            })
            .collect::<Vec<_>>();

        if !transaction_models.is_empty() {
            let query = token_transfers::Entity::insert_many(transaction_models)
                .on_conflict(
                    OnConflict::columns([
                        token_transfers::Column::Signature,
                        token_transfers::Column::BlockTime,
                        token_transfers::Column::SourceAddress,
                        token_transfers::Column::DestinationAddress,
                    ])
                    .do_nothing()
                    .to_owned(),
                )
                .build(txn.get_database_backend());
            txn.execute(query).await?;
        }
        Ok(())
    }

    pub async fn fetch_last_indexed_slot(&self) -> Option<i64> {
        loop {
            let context = blocks::Entity::find()
                .select_only()
                .column_as(Expr::col(blocks::Column::Slot).max(), "slot")
                .into_model::<SlotModel>()
                .one(&*self.db)
                .await;

            match context {
                Ok(context) => {
                    return context
                        .expect("Always expected maximum query to return a result")
                        .slot
                }
                Err(e) => {
                    log::error!("Failed to fetch current slot from database: {}", e);
                    sleep(Duration::from_secs(5));
                }
            }
        }
    }

    pub async fn index_transaction_update(
        &self,
        txn: &DatabaseTransaction,
        state_update: StateUpdate,
    ) -> Result<(), IndexerError> {
        if state_update == StateUpdate::default() {
            return Ok(());
        }
        let StateUpdate { transactions } = state_update;

        let transactions_vec = transactions.into_iter().collect::<Vec<_>>();

        debug!("indexing transaction metadatas...");
        for chunk in transactions_vec.chunks(MAX_SQL_INSERTS) {
            self.index_transactions_without_commit(txn, chunk).await?;
        }

        Ok(())
    }
}
