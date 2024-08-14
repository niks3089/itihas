use std::{
    env,
    path::{Path, PathBuf},
    sync::Mutex,
};

use api::{api::Api, config::setup_config};
use indexer::{db::Dao, parser::parse_ui_confirmed_block, types::BlockInfo};

use migration::{Migrator, MigratorTrait};
use once_cell::sync::Lazy;
use sea_orm::{DatabaseConnection, SqlxPostgresConnector};

use solana_client::{
    nonblocking::rpc_client::RpcClient, rpc_config::RpcTransactionConfig, rpc_request::RpcRequest,
};
use solana_sdk::{
    clock::Slot,
    commitment_config::{CommitmentConfig, CommitmentLevel},
};
use solana_transaction_status::{UiConfirmedBlock, UiTransactionEncoding};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    PgPool,
};
use std::sync::Arc;

const RPC_CONFIG: RpcTransactionConfig = RpcTransactionConfig {
    encoding: Some(UiTransactionEncoding::Base64),
    commitment: Some(CommitmentConfig {
        commitment: CommitmentLevel::Confirmed,
    }),
    max_supported_transaction_version: Some(0),
};

static INIT: Lazy<Mutex<Option<()>>> = Lazy::new(|| Mutex::new(None));

fn setup_logging() {
    let env_filter =
        env::var("RUST_LOG").unwrap_or("info,sqlx=error,sea_orm_migration=error".to_string());
    tracing_subscriber::fmt()
        .with_test_writer()
        .with_env_filter(env_filter)
        .init();
}

async fn run_migrations(db: &DatabaseConnection) {
    Migrator::fresh(db).await.unwrap();
}

async fn run_one_time_setup(db: &DatabaseConnection) {
    let mut init = INIT.lock().unwrap();
    if init.is_none() {
        setup_logging();
        Migrator::fresh(db).await.unwrap();
        *init = Some(())
    }
}

pub struct TestSetup {
    pub dao: Dao,
    pub api: Api,
    pub name: String,
    pub client: Arc<RpcClient>,
}

#[derive(Clone, Copy, Debug)]
pub enum Network {
    #[allow(unused)]
    Mainnet,
    #[allow(unused)]
    Devnet,
}

#[derive(Clone, Copy)]
pub struct TestSetupOptions {
    pub network: Network,
}

pub async fn setup(name: String, opts: TestSetupOptions) -> TestSetup {
    let local_db = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set");
    if !(local_db.contains("127.0.0.1") || local_db.contains("localhost")) {
        panic!("Refusing to run tests on non-local database out of caution");
    }

    let pool = setup_pg_pool(local_db.to_string()).await;
    let db_conn = Arc::new(SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone()));
    let dao = Dao::new(SqlxPostgresConnector::from_sqlx_postgres_pool(pool));

    run_one_time_setup(&db_conn).await;
    run_migrations(&db_conn).await;
    let rpc_url = match opts.network {
        Network::Mainnet => std::env::var("MAINNET_RPC_URL").unwrap(),
        Network::Devnet => std::env::var("DEVNET_RPC_URL").unwrap(),
    };
    let client = Arc::new(RpcClient::new(rpc_url.to_string()));
    let config = setup_config();
    let api = Api::new(config).await;
    TestSetup {
        name,
        dao,
        api,
        client,
    }
}

pub async fn setup_pg_pool(database_url: String) -> PgPool {
    let options: PgConnectOptions = database_url.parse().unwrap();
    PgPoolOptions::new()
        .min_connections(1)
        .connect_with(options)
        .await
        .unwrap()
}

async fn fetch_block(client: &RpcClient, slot: Slot) -> UiConfirmedBlock {
    client
        .send(RpcRequest::GetBlock, serde_json::json!([slot, RPC_CONFIG,]))
        .await
        .unwrap()
}

pub async fn cached_fetch_block(setup: &TestSetup, slot: Slot) -> BlockInfo {
    let dir = relative_project_path(&format!("tests/data/blocks/{}", setup.name));
    if !Path::new(&dir).exists() {
        std::fs::create_dir(&dir).unwrap();
    }
    let file_path = dir.join(format!("{}", slot));

    let block: UiConfirmedBlock = if file_path.exists() {
        let txn_string = std::fs::read(file_path).unwrap();
        serde_json::from_slice(&txn_string).unwrap()
    } else {
        let block = fetch_block(&setup.client, slot).await;
        std::fs::write(file_path, serde_json::to_string(&block).unwrap()).unwrap();
        block
    };
    parse_ui_confirmed_block(block, slot).unwrap()
}

pub fn trim_test_name(name: &str) -> String {
    // Remove the test_ prefix and the case suffix
    name.replace("test_", "")
        .split("::case")
        .next()
        .unwrap()
        .to_string()
}

fn relative_project_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}
