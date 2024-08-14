use sea_orm::{DatabaseConnection, SqlxPostgresConnector};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    PgPool,
};

async fn setup_pg_pool(database_url: &str, max_connections: u32) -> PgPool {
    let options: PgConnectOptions = database_url.parse().unwrap();
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect_with(options)
        .await
        .unwrap()
}

pub async fn setup_database_connection(db_url: String, max_connections: u32) -> DatabaseConnection {
    SqlxPostgresConnector::from_sqlx_postgres_pool(setup_pg_pool(&db_url, max_connections).await)
}
