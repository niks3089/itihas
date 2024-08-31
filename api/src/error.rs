use {jsonrpsee::core::Error as RpcError, jsonrpsee::types::error::CallError, thiserror::Error};

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Pagination Error. Limit should not be greater than 1000.")]
    PaginationExceededError,
    #[error("Pagination Error. No Pagination Method Selected.")]
    PaginationEmptyError,
    #[error("Pagination Error. Only one pagination parameter supported per query.")]
    PaginationError,
    #[error(
        "Paginating beyond 500000 items is not supported. Please use date based pagination instead"
    )]
    OffsetLimitExceededError,
    #[error("Server Failed to Start")]
    ServerStartError(#[from] RpcError),
    #[error("Pubkey Validation Err: {0} is invalid")]
    PubkeyValidationError(String),
    #[error("Missing or invalid configuration: ({msg})")]
    ConfigurationError { msg: String },
    #[error("Database Error: {0}")]
    DatabaseError(String),
    #[error("Transaction not found: {0}")]
    TransactionNotFound(String),
    #[error("Invalid date: {0}")]
    InvalidDate(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

impl From<sea_orm::error::DbErr> for ApiError {
    fn from(err: sea_orm::error::DbErr) -> Self {
        ApiError::DatabaseError(format!("DatabaseError: {}", err))
    }
}

impl From<ApiError> for RpcError {
    fn from(val: ApiError) -> Self {
        RpcError::Call(CallError::from_std_error(val))
    }
}
