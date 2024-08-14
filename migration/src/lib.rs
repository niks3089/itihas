pub use sea_orm_migration::prelude::*;

mod m20240802_114508_init;
mod m20240805_174804_hypertable;
mod model;
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20240802_114508_init::Migration),
            Box::new(m20240805_174804_hypertable::Migration),
        ]
    }
}
