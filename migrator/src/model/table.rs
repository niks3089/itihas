use sea_orm_migration::prelude::*;

#[derive(Copy, Clone, Iden)]
pub enum Blocks {
    Table,
    Slot,
    ParentSlot,
    BlockHeight,
    BlockTime,
}

#[derive(Copy, Clone, Iden)]
pub enum TokenTransfers {
    Table,
    Signature,
    SourceAddress,
    DestinationAddress,
    SourceAta,
    DestinationAta,
    MintAddress,
    ProgramId,
    Amount,
    Slot,
    Error,
    BlockTime,
    CreatedAt,
}
