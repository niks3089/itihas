use std::sync::Arc;

use crate::error::ApiError;
use chrono::DateTime;
use chrono::FixedOffset;
use chrono::NaiveDate;
use chrono::TimeZone;
use dao::generated::prelude::TokenTransfers;
use dao::generated::token_transfers;
use schemars::JsonSchema;
use sea_orm::ColumnTrait;
use sea_orm::DatabaseConnection;
use sea_orm::EntityTrait;
use sea_orm::Order;
use sea_orm::QueryFilter;
use sea_orm::QueryOrder;
use sea_orm::QuerySelect;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone)]
pub struct Dao {
    pub db: Arc<DatabaseConnection>,
}

pub enum Pagination {
    Keyset {
        before: Option<NaiveDate>,
        after: Option<NaiveDate>,
    },
    Page {
        page: u64,
    },
}

pub fn paginate<'db, T, C>(pagination: &Pagination, limit: u64, stmt: T, column: C) -> T
where
    T: QueryFilter + QuerySelect,
    C: ColumnTrait,
{
    let mut stmt = stmt;
    match pagination {
        Pagination::Keyset { before, after } => {
            let timezone_offset = FixedOffset::east(5 * 3600 + 30 * 60); // Example for UTC+5:30

            if let Some(before) = before {
                let before_datetime = before.and_hms(23, 59, 59);
                let before_fixed: DateTime<FixedOffset> = timezone_offset
                    .from_local_datetime(&before_datetime)
                    .unwrap();
                stmt = stmt.filter(column.lt(before_fixed));
            }

            if let Some(after) = after {
                let after_datetime = after.and_hms(0, 0, 0);
                let after_fixed: DateTime<FixedOffset> = timezone_offset
                    .from_local_datetime(&after_datetime)
                    .unwrap();
                stmt = stmt.filter(column.gt(after_fixed));
            }
        }
        Pagination::Page { page } => {
            if *page > 0 {
                stmt = stmt.offset((page - 1) * limit)
            }
        }
    }
    stmt.limit(limit)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]

pub struct TransactionSorting {
    pub sort_by: TransactionSortBy,
    pub sort_direction: Option<TransactionSortDirection>,
}

impl Default for TransactionSorting {
    fn default() -> TransactionSorting {
        TransactionSorting {
            sort_by: TransactionSortBy::Created,
            sort_direction: Some(TransactionSortDirection::default()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]

pub enum TransactionSortBy {
    #[serde(rename = "created")]
    Created,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum TransactionSortDirection {
    #[serde(rename = "asc")]
    Asc,
    #[serde(rename = "desc")]
    Desc,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct PageOptions {
    pub limit: u64,
    pub page: Option<u64>,
    pub before: Option<NaiveDate>,
    pub after: Option<NaiveDate>,
}

impl Default for TransactionSortDirection {
    fn default() -> TransactionSortDirection {
        TransactionSortDirection::Desc
    }
}

pub fn create_sorting(
    sorting: TransactionSorting,
) -> (sea_orm::query::Order, Option<token_transfers::Column>) {
    let sort_column = match sorting.sort_by {
        TransactionSortBy::Created => Some(token_transfers::Column::BlockTime),
    };
    let sort_direction = match sorting.sort_direction.unwrap_or_default() {
        TransactionSortDirection::Desc => sea_orm::query::Order::Desc,
        TransactionSortDirection::Asc => sea_orm::query::Order::Asc,
    };
    (sort_direction, sort_column)
}

impl Dao {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Dao { db }
    }

    pub fn get_db(&self) -> &DatabaseConnection {
        &self.db
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
            .one(self.get_db())
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?
            .ok_or(ApiError::TransactionNotFound(id))?;

        Ok(transaction)
    }

    // pub async fn get_transactions_by_date(
    //     &self,
    //     date: NaiveDate,
    // ) -> Result<Vec<token_transfers::Model>, ApiError> {
    //     // Convert NaiveDate to NaiveDateTime for the start and end of the day
    //     let start_of_day_naive = date.and_hms_opt(0, 0, 0).ok_or(ApiError::InvalidDate)?;
    //     let end_of_day_naive = date.and_hms_opt(23, 59, 59).ok_or(ApiError::InvalidDate)?;

    //     let timezone_offset = FixedOffset::east_opt(5 * 3600 + 30 * 60).unwrap();
    //     let start_of_day: DateTime<FixedOffset> = timezone_offset
    //         .from_local_datetime(&start_of_day_naive)
    //         .unwrap();
    //     let end_of_day: DateTime<FixedOffset> = timezone_offset
    //         .from_local_datetime(&end_of_day_naive)
    //         .unwrap();

    //     let transactions = token_transfers::Entity::find()
    //         .filter(token_transfers::Column::BlockTime.between(start_of_day, end_of_day))
    //         .order_by_asc(token_transfers::Column::BlockTime)
    //         .order_by_asc(token_transfers::Column::Signature)
    //         .all(self.get_db())
    //         .await
    //         .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

    //     Ok(transactions)
    // }

    pub async fn get_transactions_by_address(
        &self,
        source: Vec<u8>,
        destination: Option<Vec<u8>>,
        mint: Option<Vec<u8>>,
        pagination: &Pagination,
        limit: u64,
        sort_direction: Order,
        sort_by: Option<token_transfers::Column>,
    ) -> Result<Vec<token_transfers::Model>, ApiError> {
        let mut query = token_transfers::Entity::find()
            .filter(token_transfers::Column::SourceAddress.eq(source.clone()))
            .order_by_asc(token_transfers::Column::BlockTime)
            .order_by_asc(token_transfers::Column::Signature);

        if let Some(dest_address) = destination {
            query = query.filter(token_transfers::Column::DestinationAddress.eq(dest_address));
        }

        if let Some(mint_address) = mint {
            query = query.filter(token_transfers::Column::MintAddress.eq(mint_address));
        }

        if let Some(col) = sort_by {
            query = query
                .order_by(col, sort_direction.clone())
                .order_by(token_transfers::Column::BlockTime, sort_direction.clone());
        }

        let transactions = paginate(pagination, limit, query, token_transfers::Column::BlockTime)
            .all(self.get_db())
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        Ok(transactions)
    }

    pub async fn get_transactions_by_mint(
        &self,
        mint: Vec<u8>,
        pagination: &Pagination,
        limit: u64,
        sort_direction: Order,
        sort_by: Option<token_transfers::Column>,
    ) -> Result<Vec<token_transfers::Model>, ApiError> {
        let mut query = token_transfers::Entity::find()
            .filter(token_transfers::Column::MintAddress.eq(mint.clone()))
            .order_by_asc(token_transfers::Column::BlockTime)
            .order_by_asc(token_transfers::Column::Signature);

        if let Some(col) = sort_by {
            query = query
                .order_by(col, sort_direction.clone())
                .order_by(token_transfers::Column::BlockTime, sort_direction.clone());
        }

        let transactions = paginate(pagination, limit, query, token_transfers::Column::BlockTime)
            .all(self.get_db())
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        Ok(transactions)
    }
}
