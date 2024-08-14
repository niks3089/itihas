use std::{sync::Arc, thread::sleep, time::Duration};

use futures::future::join_all;
use tokio::sync::{
    mpsc::{self},
    Mutex, Notify,
};

use crate::{
    config::IndexerConfig,
    db::Dao,
    error::IndexerError,
    parser::parse_block_state_update,
    types::{BlockInfo, BlockMetadata, StateUpdate, Transaction, MAX_SQL_INSERTS},
};
use log::{debug, error, warn};

impl Messenger {}

#[derive(Debug)]
pub struct Messenger {
    config: IndexerConfig,
    transaction_sender: mpsc::UnboundedSender<Vec<Transaction>>,
    transaction_receiver: Arc<Mutex<mpsc::UnboundedReceiver<Vec<Transaction>>>>,
    block_sender: mpsc::UnboundedSender<Vec<BlockMetadata>>,
    block_receiver: Arc<Mutex<mpsc::UnboundedReceiver<Vec<BlockMetadata>>>>,
    shutdown_notify: Arc<Notify>,
}

impl Messenger {
    pub fn new(config: IndexerConfig) -> Self {
        let (transaction_sender, transaction_receiver) = mpsc::unbounded_channel();
        let (block_sender, block_receiver) = mpsc::unbounded_channel();
        let shutdown_notify = Arc::new(Notify::new());

        Messenger {
            config,
            transaction_sender,
            transaction_receiver: Arc::new(Mutex::new(transaction_receiver)),
            block_sender,
            block_receiver: Arc::new(Mutex::new(block_receiver)),
            shutdown_notify,
        }
    }

    pub fn run(self: Arc<Self>, dao: Dao) {
        let txn_rx = Arc::clone(&self.transaction_receiver);
        let block_rx = Arc::clone(&self.block_receiver);

        tokio::spawn(async move {
            let txn_worker_handles = (0..self.config.workers)
                .map(|_| {
                    tokio::spawn(
                        self.clone()
                            .transaction_worker(Arc::clone(&txn_rx), dao.clone()),
                    )
                })
                .collect::<Vec<_>>();

            let block_worker_handles = (0..self.config.workers)
                .map(|_| {
                    tokio::spawn(
                        self.clone()
                            .block_worker(Arc::clone(&block_rx), dao.clone()),
                    )
                })
                .collect::<Vec<_>>();

            join_all(txn_worker_handles).await;
            join_all(block_worker_handles).await;
        });
    }
    pub async fn send_block_batches(&self, block_batch: Vec<BlockInfo>) {
        loop {
            match self.send_block_batch(&block_batch).await {
                Ok(()) => return,
                Err(e) => {
                    let start_block = block_batch.first().unwrap().metadata.slot;
                    let end_block = block_batch.last().unwrap().metadata.slot;
                    log::error!(
                        "Failed to send block batch {}-{}. Got error {}",
                        start_block,
                        end_block,
                        e
                    );
                    sleep(Duration::from_secs(1));
                }
            }
        }
    }

    pub async fn send_block_batch(&self, block_batch: &[BlockInfo]) -> Result<(), IndexerError> {
        let block_metadatas: Vec<BlockMetadata> =
            block_batch.iter().map(|b| b.metadata.clone()).collect();
        self.send_block_metadatas(block_metadatas).await?;
        let mut state_updates = Vec::new();
        for block in block_batch {
            state_updates.push(parse_block_state_update(block)?);
        }
        self.send_transaction_update(StateUpdate::merge_updates(state_updates))
            .await?;
        Ok(())
    }

    pub async fn send_block_metadatas(
        &self,
        blocks: Vec<BlockMetadata>,
    ) -> Result<(), IndexerError> {
        for block_chunk in blocks.chunks(MAX_SQL_INSERTS) {
            let chunk = block_chunk.to_vec();
            self.block_sender
                .send(chunk)
                .map_err(|e| IndexerError::MessengerError(e.to_string()))?;
        }

        Ok(())
    }

    pub async fn send_transaction_update(
        &self,
        state_update: StateUpdate,
    ) -> Result<(), IndexerError> {
        if state_update == StateUpdate::default() {
            return Ok(());
        }
        let StateUpdate { transactions } = state_update;

        let transactions_vec = transactions.into_iter().collect::<Vec<_>>();

        debug!("sending transaction metadatas...");
        for chunk in transactions_vec.chunks(MAX_SQL_INSERTS) {
            let chunk = chunk.to_vec();
            self.transaction_sender
                .send(chunk)
                .map_err(|e| IndexerError::MessengerError(e.to_string()))?;
        }

        Ok(())
    }

    pub async fn block_worker(
        self: Arc<Self>,
        block_receiver: Arc<Mutex<mpsc::UnboundedReceiver<Vec<BlockMetadata>>>>,
        dao: Dao,
    ) {
        loop {
            tokio::select! {
                blocks = async {
                    let mut rx_lock = block_receiver.lock().await;
                    rx_lock.recv().await
                } => {
                    match blocks {
                        Some(blocks) => {
                            let block_refs: Vec<&BlockMetadata> = blocks.iter().collect();
                            if let Err(e) = dao.index_block_metadatas(block_refs).await {
                                error!("Failed to index block metadata: {:?}", e);
                            }
                        },
                        None => {
                            error!("Block receiver closed");
                            break;
                        }
                    }
                }
                _ = self.shutdown_notify.notified() => {
                    warn!("Shutdown signal received");
                    break;
                }
            }
        }
    }

    pub async fn transaction_worker(
        self: Arc<Self>,
        transaction_receiver: Arc<Mutex<mpsc::UnboundedReceiver<Vec<Transaction>>>>,
        dao: Dao,
    ) {
        loop {
            tokio::select! {
                    transactions = async {
                        let mut rx_lock = transaction_receiver.lock().await;
                        rx_lock.recv().await
                    } => {
                    match transactions {
                        Some(transaction) => {
                            if let Err(e) = dao.index_transaction(&transaction).await {
                                error!("Failed to index transaction: {:?}", e);
                            }
                        },
                        None => {
                            error!("Transaction receiver closed");
                            break;
                        }
                    }
                }
                _ = self.shutdown_notify.notified() => {
                    warn!("Shutdown signal received");
                    break;
                }
            }
        }
    }
}
