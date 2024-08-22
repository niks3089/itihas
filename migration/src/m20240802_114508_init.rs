use sea_orm_migration::{
    prelude::*,
    sea_orm::{ConnectionTrait, Statement},
};

use super::model::table::{Blocks, TokenTransfers};

#[derive(DeriveMigrationName)]
pub struct Migration;

async fn execute_sql<'a>(manager: &SchemaManager<'_>, sql: &str) -> Result<(), DbErr> {
    manager
        .get_connection()
        .execute(Statement::from_string(
            manager.get_database_backend(),
            sql.to_string(),
        ))
        .await?;
    Ok(())
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        execute_sql(
            manager,
            "
            DO $$
            BEGIN
                IF NOT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'timescaledb') THEN
                    CREATE EXTENSION timescaledb CASCADE;
                END IF;
            END $$;
            ",
        )
        .await?;

        execute_sql(
            manager,
            "
                DO $$
                DECLARE
                    type_exists BOOLEAN := EXISTS (SELECT 1 FROM pg_type WHERE typname = 'bigint2');
                BEGIN
                    IF NOT type_exists THEN
                        CREATE DOMAIN bigint2 AS numeric(20, 0);
                    END IF;
                END $$;
                ",
        )
        .await?;

        manager
            .create_table(
                Table::create()
                    .table(Blocks::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Blocks::Slot).big_integer().not_null())
                    .col(ColumnDef::new(Blocks::ParentSlot).big_integer().not_null())
                    .col(ColumnDef::new(Blocks::BlockHeight).big_integer().not_null())
                    .col(ColumnDef::new(Blocks::BlockTime).big_integer().not_null())
                    .primary_key(
                        Index::create()
                            .name("pk_blocks")
                            .col(Blocks::Slot)
                            .col(TokenTransfers::BlockTime),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(TokenTransfers::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(TokenTransfers::Signature)
                            .binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TokenTransfers::SourceAddress)
                            .binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TokenTransfers::TokenType)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TokenTransfers::DestinationAddress)
                            .binary()
                            .not_null(),
                    )
                    .col(ColumnDef::new(TokenTransfers::SourceAta).binary())
                    .col(ColumnDef::new(TokenTransfers::DestinationAta).binary())
                    .col(ColumnDef::new(TokenTransfers::MintAddress).binary())
                    .col(
                        ColumnDef::new(TokenTransfers::Slot)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TokenTransfers::Amount)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(TokenTransfers::Error).text())
                    .col(
                        ColumnDef::new(TokenTransfers::BlockTime)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TokenTransfers::CreatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .primary_key(
                        Index::create()
                            .name("pk_token_transfers")
                            .col(TokenTransfers::Signature)
                            .col(TokenTransfers::SourceAddress)
                            .col(TokenTransfers::DestinationAddress)
                            .col(TokenTransfers::BlockTime),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Blocks::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(TokenTransfers::Table).to_owned())
            .await?;

        Ok(())
    }
}
