use std::str::FromStr;

use crate::{api::Api, error::ApiError, types::Transaction};
use open_rpc_derive::document_rpc;
use open_rpc_schema::document::OpenrpcDocument;
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use solana_sdk::pubkey::Pubkey;

use super::{ApiContract, GetTransactionsByAddress};

use async_trait::async_trait;

pub fn validate_pubkey(str_pubkey: String) -> Result<Pubkey, ApiError> {
    Pubkey::from_str(&str_pubkey).map_err(|_| ApiError::PubkeyValidationError(str_pubkey))
}

#[document_rpc]
#[async_trait]
impl ApiContract for Api {
    // Liveness probe determines if the pod is healthy. Kubernetes will restart the pod if this fails.
    async fn liveness(self: &Api) -> Result<(), ApiError> {
        Ok(())
    }

    // Readiness probe determines if the pod has capacity to accept traffic. Kubernetes will not route traffic to this pod if this fails.
    // We are essentially checking if there are DB connections available.
    async fn readiness(self: &Api) -> Result<(), ApiError> {
        self.dao
            .db
            .execute(Statement::from_string(
                DbBackend::Postgres,
                "SELECT 1".to_string(),
            ))
            .await?;
        Ok(())
    }

    async fn get_transactions_by_address(
        self: &Api,
        payload: GetTransactionsByAddress,
    ) -> Result<Vec<Transaction>, ApiError> {
        let GetTransactionsByAddress {
            source,
            destination,
        } = payload;

        let source = validate_pubkey(source)?.to_bytes().to_vec();
        let destination = if let Some(dest) = destination {
            Some(validate_pubkey(dest)?.to_bytes().to_vec())
        } else {
            None
        };

        let models = self
            .dao
            .get_transactions_by_address(source, destination)
            .await?;
        let transactions: Vec<Transaction> = models.into_iter().map(Transaction::from).collect();
        Ok(transactions)
    }
}
