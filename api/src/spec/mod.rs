use crate::db::TransactionSorting;
use crate::error::ApiError;
use crate::types::Transaction;
use async_trait::async_trait;
use open_rpc_derive::{document_rpc, rpc};
use open_rpc_schema::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod api_impl;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetTransactionsByAddress {
    pub source_address: Option<String>,
    pub destination_address: Option<String>,
    pub mint_address: Option<String>,
    pub limit: Option<u32>,
    pub page: Option<u32>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub sort_by: Option<TransactionSorting>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, JsonSchema)]
#[serde(default)]
pub struct TransactionList {
    pub total: u32,
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    pub items: Vec<Transaction>,
}

#[document_rpc]
#[async_trait]
pub trait ApiContract: Send + Sync + 'static {
    async fn liveness(&self) -> Result<(), ApiError>;
    async fn readiness(&self) -> Result<(), ApiError>;

    #[rpc(
        name = "getTransactionsByAddress",
        params = "named",
        summary = "Get all transactions for an address"
    )]
    async fn get_transactions_by_address(
        &self,
        payload: GetTransactionsByAddress,
    ) -> Result<TransactionList, ApiError>;
}
