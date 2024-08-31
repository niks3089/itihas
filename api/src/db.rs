use std::sync::Arc;

use crate::error::ApiError;
use chrono::DateTime;
use chrono::NaiveDate;
use chrono::TimeZone;
use chrono::Utc;
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

pub fn paginate<T, C>(pagination: &Pagination, limit: u64, stmt: T, column: C) -> T
where
    T: QueryFilter + QuerySelect,
    C: ColumnTrait,
{
    let mut stmt = stmt;
    match pagination {
        Pagination::Keyset { before, after } => {
            if let Some(before) = before {
                let before_datetime = before.and_hms_opt(23, 59, 59).unwrap();
                let before_utc: DateTime<Utc> = Utc.from_utc_datetime(&before_datetime);
                stmt = stmt.filter(column.lt(before_utc));
            }

            if let Some(after) = after {
                let after_datetime = after.and_hms_opt(0, 0, 0).unwrap();
                let after_utc: DateTime<Utc> = Utc.from_utc_datetime(&after_datetime);
                stmt = stmt.filter(column.gt(after_utc));
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
            sort_by: TransactionSortBy::Slot,
            sort_direction: Some(TransactionSortDirection::default()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]

pub enum TransactionSortBy {
    #[serde(rename = "created")]
    Created,
    #[serde(rename = "slot")]
    Slot,
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
        TransactionSortBy::Slot => Some(token_transfers::Column::Slot),
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

    pub async fn get_transactions_by_address(
        &self,
        source: Option<Vec<u8>>,
        destination: Option<Vec<u8>>,
        mint: Option<Vec<u8>>,
        pagination: &Pagination,
        limit: u64,
        sort_direction: Order,
        sort_by: Option<token_transfers::Column>,
    ) -> Result<Vec<token_transfers::Model>, ApiError> {
        let mut query = token_transfers::Entity::find();

        if let Some(source_address) = source {
            query = query.filter(token_transfers::Column::SourceAddress.eq(source_address));
        }

        if let Some(dest_address) = destination {
            query = query.filter(token_transfers::Column::DestinationAddress.eq(dest_address));
        }

        if let Some(mint_address) = mint {
            query = query.filter(token_transfers::Column::MintAddress.eq(mint_address));
        }

        if let Some(col) = sort_by {
            query = query
                .order_by(col, sort_direction.clone())
                .order_by(token_transfers::Column::Slot, sort_direction.clone());
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
            .filter(token_transfers::Column::MintAddress.eq(mint.clone()));

        if let Some(col) = sort_by {
            query = query
                .order_by(col, sort_direction.clone())
                .order_by(token_transfers::Column::Slot, sort_direction.clone());
        }

        let transactions = paginate(pagination, limit, query, token_transfers::Column::BlockTime)
            .all(self.get_db())
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        Ok(transactions)
    }
}
