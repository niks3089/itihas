use sea_orm::Transaction;
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum IndexerError {
    #[error("Network Error: {0}")]
    BatchInitNetworkingError(String),
    #[error("Missing or invalid configuration: ({msg})")]
    ConfigurationError { msg: String },
    #[error("Serializaton error: {0}")]
    SerializatonError(String),
    #[error("Messenger error; {0}")]
    MessengerError(String),
    #[error("Parser error: {0}")]
    ParserError(String),
    #[error("Database Error: {0}")]
    DatabaseError(String),
    #[error("Cache Storage Write Error: {0}")]
    CacheStorageWriteError(String),
    #[error("AssetIndex Error {0}")]
    AssetIndexError(String),
}

impl From<sea_orm::error::DbErr> for IndexerError {
    fn from(err: sea_orm::error::DbErr) -> Self {
        IndexerError::DatabaseError(format!("DatabaseError: {}", err))
    }
}

impl From<tokio::sync::mpsc::error::SendError<Vec<Transaction>>> for IndexerError {
    fn from(err: tokio::sync::mpsc::error::SendError<Vec<Transaction>>) -> Self {
        IndexerError::MessengerError((format!("MessagengerError: {}", err)).to_string())
    }
}

impl From<solana_sdk::pubkey::ParsePubkeyError> for IndexerError {
    fn from(err: solana_sdk::pubkey::ParsePubkeyError) -> Self {
        IndexerError::SerializatonError(format!("ParsePubkeyError: {}", err))
    }
}
