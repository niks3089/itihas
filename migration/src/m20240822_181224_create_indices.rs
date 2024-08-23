use sea_orm_migration::prelude::*;

use super::model::table::TokenTransfers;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("idx_token_transfers_mint_address")
                    .table(TokenTransfers::Table)
                    .col(TokenTransfers::MintAddress)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_token_transfers_slot")
                    .table(TokenTransfers::Table)
                    .col(TokenTransfers::Slot)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_token_transfers_mint_address")
                    .table(TokenTransfers::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_token_transfers_slot")
                    .table(TokenTransfers::Table)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
