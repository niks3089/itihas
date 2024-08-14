use function_name::named;
use rstest::rstest;

use chrono::NaiveDate;
use insta::assert_json_snapshot;
use serial_test::serial;

use crate::setup::cached_fetch_block;

#[named]
#[rstest]
#[tokio::test]
#[serial]
async fn test_get_transaction() {
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

    // We don't do this because rpc could timeout for tests
    // let block = fetch_block_with_using_arc(setup.client.clone(), 317418346)
    //     .await
    //     .unwrap();
    let block = cached_fetch_block(&setup, 317418346).await;
    let _ = setup.dao.index_block(&block).await;

    for (i, txn_signature) in txns.iter().enumerate() {
        let parsed_transaction = setup
            .api
            .get_transaction_by_id(txn_signature.to_string())
            .await
            .unwrap();
        assert_json_snapshot!(
            format!("{}-{}-transaction", name.clone(), i),
            parsed_transaction
        );
    }

    let res = setup
        .api
        .get_transaction_by_id("unknown_txn".to_string())
        .await;

    assert!(res.is_err());
}

#[named]
#[rstest]
#[tokio::test]
#[serial]
async fn test_get_transactions_by_date() {
    use crate::setup::{setup, trim_test_name, Network, TestSetupOptions};

    let name = trim_test_name(function_name!());
    let setup = setup(
        name.clone(),
        TestSetupOptions {
            network: Network::Devnet,
        },
    )
    .await;
    // We don't do this because rpc could timeout for tests
    // let block = fetch_block_with_using_arc(setup.client.clone(), 289867138)
    //     .await
    //     .unwrap();
    let block = cached_fetch_block(&setup, 317418346).await;
    setup.dao.index_block(&block).await.unwrap();

    let naive_date_time =
        NaiveDate::parse_from_str("08/08/2024", "%d/%m/%Y").expect("Failed to parse date");
    let txns = setup
        .api
        .get_transactions_by_date(naive_date_time)
        .await
        .unwrap();

    assert_eq!(1, txns.len());

    let naive_date_time =
        NaiveDate::parse_from_str("05/04/2024", "%d/%m/%Y").expect("Failed to parse date");
    let txns = setup
        .api
        .get_transactions_by_date(naive_date_time)
        .await
        .unwrap();

    assert_eq!(0, txns.len());
}
