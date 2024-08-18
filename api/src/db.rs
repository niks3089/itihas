use std::sync::Arc;

use crate::error::ApiError;
use chrono::DateTime;
use chrono::FixedOffset;
use chrono::NaiveDate;
use chrono::TimeZone;
use dao::generated::prelude::TokenTransfers;
use dao::generated::token_transfers;
use sea_orm::ColumnTrait;
use sea_orm::DatabaseConnection;
use sea_orm::EntityTrait;
use sea_orm::QueryFilter;
use sea_orm::QueryOrder;

#[derive(Clone)]
pub struct Dao {
    pub db: Arc<DatabaseConnection>,
}

impl Dao {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Dao { db }
    }

    pub async fn get_transaction_by_id(
        &self,
        id: String,
    ) -> Result<token_transfers::Model, ApiError> {
        let signature_bytes = bs58::decode(id.clone())
            .into_vec()
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        let transaction = TokenTransfers::find()
            .filter(token_transfers::Column::Signature.eq(signature_bytes))
            .one(&*self.db)
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?
            .ok_or(ApiError::TransactionNotFound(id))?;

        Ok(transaction)
    }

    pub async fn get_transactions_by_date(
        &self,
        date: NaiveDate,
    ) -> Result<Vec<token_transfers::Model>, ApiError> {
        // Convert NaiveDate to NaiveDateTime for the start and end of the day
        let start_of_day_naive = date.and_hms_opt(0, 0, 0).ok_or(ApiError::InvalidDate)?;
        let end_of_day_naive = date.and_hms_opt(23, 59, 59).ok_or(ApiError::InvalidDate)?;

        let timezone_offset = FixedOffset::east_opt(5 * 3600 + 30 * 60).unwrap();
        let start_of_day: DateTime<FixedOffset> = timezone_offset
            .from_local_datetime(&start_of_day_naive)
            .unwrap();
        let end_of_day: DateTime<FixedOffset> = timezone_offset
            .from_local_datetime(&end_of_day_naive)
            .unwrap();

        let transactions = token_transfers::Entity::find()
            .filter(token_transfers::Column::BlockTime.between(start_of_day, end_of_day))
            .order_by_asc(token_transfers::Column::BlockTime)
            .order_by_asc(token_transfers::Column::Signature)
            .all(&*self.db)
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        Ok(transactions)
    }

    pub async fn get_transactions_by_address(
        &self,
        source: Vec<u8>,
        destination: Option<Vec<u8>>,
    ) -> Result<Vec<token_transfers::Model>, ApiError> {
        let mut query = token_transfers::Entity::find()
            .filter(token_transfers::Column::SrcAddress.eq(source.clone()))
            .order_by_asc(token_transfers::Column::BlockTime)
            .order_by_asc(token_transfers::Column::Signature);

        if let Some(dest_address) = destination {
            query = query.filter(token_transfers::Column::DestAddress.eq(dest_address));
        }

        let transactions = query
            .all(&*self.db)
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        Ok(transactions)
    }
}
