use crate::{
    api::{validate_pubkey, Api},
    db::create_sorting,
    error::ApiError,
    types::Transaction,
};
use open_rpc_derive::document_rpc;
use open_rpc_schema::document::OpenrpcDocument;
use sea_orm::{ConnectionTrait, DbBackend, Statement};

use super::{ApiContract, GetTransactionsByAddress, TransactionList};

use async_trait::async_trait;

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
    ) -> Result<TransactionList, ApiError> {
        let GetTransactionsByAddress {
            source_address,
            destination_address,
            mint_address,
            before,
            after,
            limit,
            page,
            sort_by,
        } = payload;

        if source_address.is_none() && destination_address.is_none() && mint_address.is_none() {
            return Err(ApiError::InvalidInput(
                "source_address, destination_address or mint_address must be provided".to_string(),
            ));
        }

        let source = if let Some(source) = source_address {
            Some(validate_pubkey(source)?.to_bytes().to_vec())
        } else {
            None
        };

        let destination = if let Some(dest) = destination_address {
            Some(validate_pubkey(dest)?.to_bytes().to_vec())
        } else {
            None
        };

        let mint = if let Some(mint) = mint_address {
            Some(validate_pubkey(mint)?.to_bytes().to_vec())
        } else {
            None
        };

        let page = self.validate_pagination(&limit, &page, &before, &after)?;
        let pagination = self.create_pagination(page.clone())?;
        let (sort_direction, sort_column) = create_sorting(sort_by.unwrap_or_default());

        let models = self
            .dao
            .get_transactions_by_address(
                source,
                destination,
                mint,
                &pagination,
                page.limit,
                sort_direction,
                sort_column,
            )
            .await?;
        let transactions: Vec<Transaction> = models.into_iter().map(Transaction::from).collect();
        Ok(Api::build_transaction_response(
            transactions,
            page.limit,
            &pagination,
        ))
    }
}
