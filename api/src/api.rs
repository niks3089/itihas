use crate::{db::Dao, error::ApiError, types::Transaction};
use chrono::NaiveDate;
use common::db::setup_database_connection;

use crate::config::ApiConfig;

pub struct Api {
    pub config: ApiConfig,
    pub dao: Dao,
}

impl Api {
    pub async fn new(config: ApiConfig) -> Self {
        Api {
            config: config.clone(),
            dao: Dao::new(
                setup_database_connection(config.get_database_url(), config.max_connections)
                    .await
                    .into(),
            ),
        }
    }

    pub async fn get_transaction_by_id(&self, id: String) -> Result<Transaction, ApiError> {
        let transaction = self.dao.get_transaction_by_id(id).await?;
        Ok(transaction.into())
    }

    pub async fn get_transactions_by_date(
        &self,
        date: NaiveDate,
    ) -> Result<Vec<Transaction>, ApiError> {
        let transactions = self.dao.get_transactions_by_date(date).await?;
        let transactions: Vec<Transaction> =
            transactions.into_iter().map(Transaction::from).collect();
        Ok(transactions)
    }
}
