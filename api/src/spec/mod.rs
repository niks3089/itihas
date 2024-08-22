use crate::error::ApiError;
use crate::types::Transaction;
use async_trait::async_trait;
use open_rpc_derive::{document_rpc, rpc};
use open_rpc_schema::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod api_impl;
pub use api_impl::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetTransactionsByAddress {
    pub source: String,
    pub destination: Option<String>,
    pub mint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetTransactionsByMint {
    pub mint: String,
}

#[document_rpc]
#[async_trait]
pub trait ApiContract: Send + Sync + 'static {
    async fn liveness(&self) -> Result<(), ApiError>;
    async fn readiness(&self) -> Result<(), ApiError>;

    #[rpc(
        name = "getTransactionsByAddress",
        params = "named",
        summary = "Get all transactions for a source account address"
    )]
    async fn get_transactions_by_address(
        &self,
        payload: GetTransactionsByAddress,
    ) -> Result<Vec<Transaction>, ApiError>;

    #[rpc(
        name = "getTransactionsByMint",
        params = "named",
        summary = "Get all transactions for a particular mint account address"
    )]
    async fn get_transactions_by_mint(
        &self,
        payload: GetTransactionsByMint,
    ) -> Result<Vec<Transaction>, ApiError>;
}
