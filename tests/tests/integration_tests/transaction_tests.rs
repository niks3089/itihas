use api::spec::{ApiContract, GetTransactionsByAddress};
use function_name::named;
use rstest::rstest;

use insta::assert_json_snapshot;
use serial_test::serial;

use crate::setup::cached_fetch_block;

#[named]
#[rstest]
#[tokio::test]
#[serial]
async fn test_get_transaction_by_address() {
    use crate::setup::{setup, trim_test_name, Network, TestSetupOptions};

    let name = trim_test_name(function_name!());
    let setup = setup(
        name.clone(),
        TestSetupOptions {
            network: Network::Mainnet,
        },
    )
    .await;

    let block = cached_fetch_block(&setup, 285941932).await;
    let _ = setup.dao.index_block(&block).await;
    let payload = GetTransactionsByAddress {
        source: Some("BhW85ig2dHu5tV6sCs7ps5UyPCLSjfXgEsfVAX82yXnb".to_string()),
        page: Some(1),
        destination: None,
        mint: None,
        before: None,
        after: None,
        limit: None,
        sort_by: None,
    };

    let parsed_transaction = setup
        .api
        .get_transactions_by_address(payload.clone())
        .await
        .unwrap();
    assert_json_snapshot!(
        format!("{}-{}-transaction-with-src", name.clone(), 1),
        parsed_transaction
    );

    let payload = GetTransactionsByAddress {
        source: Some("BhW85ig2dHu5tV6sCs7ps5UyPCLSjfXgEsfVAX82yXnb".to_string()),
        destination: Some("4HHZV2LRBQD5CJnYgMTzPeKcS2nnTT8szeh2svBWQ89m".to_string()),
        mint: None,
        page: Some(1),
        before: None,
        after: None,
        limit: None,
        sort_by: None,
    };

    let parsed_transaction = setup
        .api
        .get_transactions_by_address(payload.clone())
        .await
        .unwrap();
    assert_json_snapshot!(
        format!("{}-{}-transaction-with-dest", name.clone(), 1),
        parsed_transaction
    );

    let payload = GetTransactionsByAddress {
        source: Some("BhW85ig2dHu5tV6sCs7ps5UyPCLSjfXgEsfVAX82yXnb".to_string()),
        destination: None,
        mint: None,
        page: Some(2),
        before: None,
        after: None,
        limit: Some(5),
        sort_by: None,
    };

    let parsed_transaction = setup
        .api
        .get_transactions_by_address(payload.clone())
        .await
        .unwrap();
    assert_json_snapshot!(
        format!("{}-{}-transaction-with-page", name.clone(), 1),
        parsed_transaction
    );

    let payload = GetTransactionsByAddress {
        source: Some("HUe9Gfu8DMhY4Dj9A56N9muZg7euoFcXQskVAAfJpgEw".to_string()),
        destination: None,
        mint: None,
        page: Some(1),
        before: None,
        after: None,
        limit: None,
        sort_by: None,
    };

    let parsed_transaction = setup
        .api
        .get_transactions_by_address(payload.clone())
        .await
        .unwrap();
    assert_json_snapshot!(
        format!("{}-{}-transaction-unknown", name.clone(), 1),
        parsed_transaction
    );

    let payload = GetTransactionsByAddress {
        source: None,
        destination: None,
        mint: Some("AmeroCaeKg55p6J8d1y2R4t9taqgn3TH4BARgzQJyHvd".to_string()),
        page: Some(1),
        before: None,
        after: None,
        limit: None,
        sort_by: None,
    };

    let parsed_transaction = setup
        .api
        .get_transactions_by_address(payload.clone())
        .await
        .unwrap();
    assert_json_snapshot!(
        format!("{}-{}-transaction-mint", name.clone(), 1),
        parsed_transaction
    );
}
