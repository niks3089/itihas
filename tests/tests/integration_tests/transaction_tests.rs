use api::spec::{ApiContract, GetTransactionsByAddress, GetTransactionsByMint};
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
            network: Network::Devnet,
        },
    )
    .await;

    let txns = [
        "4gmMdnaUkDUySPtv7W3WdyPAnLtHrE4dfwkcZtGm9QCNxKNNx2RbwgATDyM95bS9V3GMbpKu3pab1XPcKvSgtDmN",
    ];

    let block = cached_fetch_block(&setup, 317418346).await;
    let _ = setup.dao.index_block(&block).await;
    let payload = GetTransactionsByAddress {
        source: "5oTHwHSgkX2kMP88aRyy2Cvi8SqhRv7DJFbacRy9upCm".to_string(),
        page: Some(1),
        destination: None,
        mint: None,
        before: None,
        after: None,
        limit: None,
        sort_by: None,
    };

    for (i, _txn_signature) in txns.iter().enumerate() {
        let parsed_transaction = setup
            .api
            .get_transactions_by_address(payload.clone())
            .await
            .unwrap();
        assert_json_snapshot!(
            format!("{}-{}-transaction-with-src", name.clone(), i),
            parsed_transaction
        );
    }

    let payload = GetTransactionsByAddress {
        source: "5oTHwHSgkX2kMP88aRyy2Cvi8SqhRv7DJFbacRy9upCm".to_string(),
        destination: Some("HUe9Gfu8DMhY4Dj9A56N9muZg7euoFcXQskVAAfJpgEw".to_string()),
        mint: None,
        page: Some(1),
        before: None,
        after: None,
        limit: None,
        sort_by: None,
    };

    for (i, _txn_signature) in txns.iter().enumerate() {
        let parsed_transaction = setup
            .api
            .get_transactions_by_address(payload.clone())
            .await
            .unwrap();
        assert_json_snapshot!(
            format!("{}-{}-transaction-with-dest", name.clone(), i),
            parsed_transaction
        );
    }

    let payload = GetTransactionsByAddress {
        source: "HUe9Gfu8DMhY4Dj9A56N9muZg7euoFcXQskVAAfJpgEw".to_string(),
        destination: None,
        mint: None,
        page: Some(1),
        before: None,
        after: None,
        limit: None,
        sort_by: None,
    };

    for (i, _txn_signature) in txns.iter().enumerate() {
        let parsed_transaction = setup
            .api
            .get_transactions_by_address(payload.clone())
            .await
            .unwrap();
        assert_json_snapshot!(
            format!("{}-{}-transaction-unknown", name.clone(), i),
            parsed_transaction
        );
    }
}

#[named]
#[rstest]
#[tokio::test]
#[serial]
async fn test_get_transaction_by_mint() {
    use crate::setup::{setup, trim_test_name, Network, TestSetupOptions};

    let name = trim_test_name(function_name!());
    let setup = setup(
        name.clone(),
        TestSetupOptions {
            network: Network::Mainnet,
        },
    )
    .await;

    let txns = [
        "2JmjebsWyiFZKzPZQV84RNMrHgTYikSCiXBdhpY1VEeNtKfVCLrZqx3PUuNbUtebhKw5hqso6DaXQMgHfmQtpHuJ",
        "jmcGKkcKiDDVmZAnUFd8Lv3ryW3TpuzzWqXyUYpGuc5dREB3hXLxaxArW4g1Mreh8P9JstDaHpEnBSfaVcNWWSH",
    ];

    let block = cached_fetch_block(&setup, 285285974).await;
    let _ = setup.dao.index_block(&block).await;
    let payload = GetTransactionsByMint {
        mint: "5oTHwHSgkX2kMP88aRyy2Cvi8SqhRv7DJFbacRy9upCm".to_string(),
        page: Some(1),
        before: None,
        after: None,
        limit: None,
        sort_by: None,
    };

    for (i, _txn_signature) in txns.iter().enumerate() {
        let parsed_transaction = setup
            .api
            .get_transactions_by_mint(payload.clone())
            .await
            .unwrap();
        assert_json_snapshot!(
            format!("{}-{}-transaction-with-mint", name.clone(), i),
            parsed_transaction
        );
    }
}
