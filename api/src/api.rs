use std::str::FromStr;

use crate::{
    db::{Dao, PageOptions, Pagination},
    error::ApiError,
    spec::TransactionList,
    types::Transaction,
};
use chrono::NaiveDate;
use common::db::setup_database_connection;
use solana_sdk::pubkey::Pubkey;

use crate::config::ApiConfig;

pub fn validate_pubkey(str_pubkey: String) -> Result<Pubkey, ApiError> {
    Pubkey::from_str(&str_pubkey).map_err(|_| ApiError::PubkeyValidationError(str_pubkey))
}

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

    pub fn create_pagination(&self, page_opt: PageOptions) -> Result<Pagination, ApiError> {
        match (
            page_opt.before.as_ref(),
            page_opt.after.as_ref(),
            page_opt.page,
        ) {
            (_, _, None) => Ok(Pagination::Keyset {
                before: page_opt.before,
                after: page_opt.after,
            }),
            (None, None, Some(p)) => Ok(Pagination::Page { page: p }),
            _ => Err(ApiError::PaginationError),
        }
    }
    pub fn validate_pagination(
        &self,
        limit: &Option<u32>,
        page: &Option<u32>,
        before: &Option<String>,
        after: &Option<String>,
    ) -> Result<PageOptions, ApiError> {
        let mut page_opt = PageOptions::default();

        if let Some(limit) = limit {
            if *limit > 1000 {
                return Err(ApiError::PaginationExceededError);
            }
        }

        if let Some(page) = page {
            if *page == 0 {
                return Err(ApiError::PaginationEmptyError);
            }

            if before.is_some() || after.is_some() {
                return Err(ApiError::PaginationError);
            }

            let current_limit = limit.unwrap_or(1000);
            let offset = (*page - 1) * current_limit;
            if offset > 500_000 {
                return Err(ApiError::OffsetLimitExceededError);
            }
        }

        if let Some(before) = before {
            match NaiveDate::parse_from_str(before, "%d/%m/%Y") {
                Ok(date) => page_opt.before = Some(date),
                Err(_) => return Err(ApiError::InvalidDate("before".to_string())),
            }
        }

        if let Some(after) = after {
            match NaiveDate::parse_from_str(after, "%d/%m/%Y") {
                Ok(date) => page_opt.after = Some(date),
                Err(_) => return Err(ApiError::InvalidDate("after".to_string())),
            }
        }

        page_opt.limit = limit.map(|x| x as u64).unwrap_or(1000);
        page_opt.page = page.map(|x| x as u64);
        Ok(page_opt)
    }

    pub fn build_transaction_response(
        transactions: Vec<Transaction>,
        limit: u64,
        pagination: &Pagination,
    ) -> TransactionList {
        let total = transactions.len() as u32;
        let (page, before, after) = match pagination {
            Pagination::Keyset { before, after } => {
                let bef = before.map(|x| x.format("%d/%m/%Y").to_string());
                let aft = after.map(|x| x.format("%d/%m/%Y").to_string());
                (None, bef, aft)
            }
            Pagination::Page { page } => (Some(*page), None, None),
        };

        TransactionList {
            total,
            limit: limit as u32,
            page: page.map(|x| x as u32),
            before,
            after,
            items: transactions,
        }
    }
}
