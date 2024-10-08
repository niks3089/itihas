//! SeaORM Entity. Generated by sea-orm-codegen 0.9.3

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Default, Debug, DeriveEntity)]
pub struct Entity;

impl EntityName for Entity {
    fn table_name(&self) -> &str {
        "token_transfers"
    }
}

#[derive(Clone, Debug, PartialEq, DeriveModel, DeriveActiveModel, Serialize, Deserialize)]
pub struct Model {
    pub signature: Vec<u8>,
    pub source_address: Vec<u8>,
    pub program_id: Vec<u8>,
    pub destination_address: Vec<u8>,
    pub source_ata: Option<Vec<u8>>,
    pub destination_ata: Option<Vec<u8>>,
    pub mint_address: Option<Vec<u8>>,
    pub slot: i64,
    pub amount: i64,
    pub error: Option<String>,
    pub block_time: DateTimeWithTimeZone,
    pub created_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
pub enum Column {
    Signature,
    SourceAddress,
    ProgramId,
    DestinationAddress,
    SourceAta,
    DestinationAta,
    MintAddress,
    Slot,
    Amount,
    Error,
    BlockTime,
    CreatedAt,
}

#[derive(Copy, Clone, Debug, EnumIter, DerivePrimaryKey)]
pub enum PrimaryKey {
    Signature,
    SourceAddress,
    DestinationAddress,
    BlockTime,
}

impl PrimaryKeyTrait for PrimaryKey {
    type ValueType = (Vec<u8>, Vec<u8>, Vec<u8>, DateTimeWithTimeZone);
    fn auto_increment() -> bool {
        false
    }
}

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {}

impl ColumnTrait for Column {
    type EntityName = Entity;
    fn def(&self) -> ColumnDef {
        match self {
            Self::Signature => ColumnType::Binary.def(),
            Self::SourceAddress => ColumnType::Binary.def(),
            Self::ProgramId => ColumnType::Binary.def(),
            Self::DestinationAddress => ColumnType::Binary.def(),
            Self::SourceAta => ColumnType::Binary.def().null(),
            Self::DestinationAta => ColumnType::Binary.def().null(),
            Self::MintAddress => ColumnType::Binary.def().null(),
            Self::Slot => ColumnType::BigInteger.def(),
            Self::Amount => ColumnType::BigInteger.def(),
            Self::Error => ColumnType::Text.def().null(),
            Self::BlockTime => ColumnType::TimestampWithTimeZone.def(),
            Self::CreatedAt => ColumnType::DateTime.def(),
        }
    }
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        panic!("No RelationDef")
    }
}

impl ActiveModelBehavior for ActiveModel {}
