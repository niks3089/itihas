use sea_orm::DatabaseBackend;
use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, Statement};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "SELECT create_hypertable('token_transfers', 'block_time');".to_string(),
            ))
            .await?;
        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "SELECT add_retention_policy('token_transfers', INTERVAL '3 months');".to_string(),
            ))
            .await?;
        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "SELECT create_hypertable('blocks', 'block_time');".to_string(),
            ))
            .await?;
        // manager
        //     .get_connection()
        //     .execute(Statement::from_string(
        //         DatabaseBackend::Postgres,
        //         "SELECT add_retention_policy('blocks', INTERVAL '3 months');".to_string(),
        //     ))
        //     .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
