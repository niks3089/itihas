# Itihas: Transaction Indexer 

## Project Composition 

The project is composed of a single workspace that includes the following packages:

- **api**: REST API 
- **common**: Common package shared among other packages 
- **dao**: Sea-orm generated types 
- **migration**: Package to run migrations on the database
- **indexer**: Polls and indexes blocks to the database 
- **tests**: Integration test package

## Running Locally 

Requires `Postgres` installed with `TimescaleDB` enabled.

### Running Migrations
To run migrations, use:
```sh
DATABASE_URL=postgres://postgres@localhost/txn cargo run -- up
```

### Running the Indexer 

```
export INDEXER_DATABASE_CONFIG='{url="postgres://postgres@localhost/txn"}'
export INDEXER_RPC_CONFIG='{url="https://rpc-devnet.helius.xyz?api-key=<apikey>"}'
cargo run --bin indexer
```

### Running the API
```
export APP_DATABASE_URL=postgres://ingest@localhost/txn
cargo run --bin api

```

## Data Indexing 

Indexing uses the RPC to continuously poll for blocks, parse the transactions, and index them onto the transaction table. The block to start fetching can be specified via config, and if that block or future blocks are already indexed, it fetches from newer blocks and skips the ones that are indexed.

To keep it simple, I am only processing transactions from token and token 2022 accounts and parsing the src/destination of the accounts onto the same transaction table. This can modified to index all the transactions and instructions easily if required. 

### Threading Model for Indexing 

#### Poller Threading Model
The poller in this application is responsible for continuously fetching and indexing new blocks from the blockchain. It operates in an asynchronous, multi-threaded environment using the Tokio runtime.

1. **Asynchronous Task Spawning**
   - The core functionality of the poller is encapsulated in an asynchronous function `continuously_index_new_blocks`.
   - This function is executed as a background task using `tokio::task::spawn`, allowing it to run concurrently with other tasks without blocking the main thread.

2. **Block Stream Generation**
   - The poller generates a stream of blocks to be indexed using the `load_block_stream` method.
   - This method calls `get_poller_block_stream`, which creates an asynchronous stream of blocks fetched from the blockchain.
   - Fetching Blocks: The stream fetches blocks from the RPC client in batches, starting from the last indexed block up to the current block height.
   - Concurrency: To optimize performance, the poller fetches multiple blocks concurrently, controlled by the `max_concurrent_block_fetches` configuration parameter.

3. **Asynchronous Processing Loop**
   - The spawned task runs an infinite loop where it processes each block from the stream as it becomes available.
   - Backfilling Historical Blocks: When the poller starts, it may need to backfill historical blocks that were not indexed previously. It calculates the number of blocks to backfill and processes them until it catches up to the current block height. If the last slot config is 0, it fetches the most recent block on chain. 
   - Real-time Indexing: Once the backfilling is complete, the poller switches to real-time indexing, processing each new block as it is produced by the blockchain. Backfilling can turned off during dev but will be essential in prod envs so that if the process restarts, there will be no gaps in the block retrieval 

4. **Transaction Handling**
   - For each block, the poller performs the following steps:
     - Indexing: The block is indexed by calling the `index_block_batches` method on the DAO.
     - Logging: Progress is logged periodically to provide visibility into the indexing process.

5. **Ensuring Consistency**
   - The poller maintains consistency by keeping track of the last indexed slot to avoid reprocessing the same blocks.
   - It uses a combination of sequential and concurrent operations to balance performance and resource usage.

After the polling is done, we don't want the polling thread to spend time indexing data, so we use a messenger which is an mpsc (multi-producer, single-consumer) model.

### Messenger Model 

The Messenger is responsible for handling the distribution and processing of block and transaction data in an asynchronous, multi-threaded environment using the Tokio runtime.

1. **Initialization and Channels**
   - The Messenger is initialized with configuration settings and creates channels for communication between different parts of the system.
   - Transaction Channels: `transaction_sender` and `transaction_receiver` are used to send and receive batches of transactions.
   - Block Channels: `block_sender` and `block_receiver` are used to send and receive batches of block metadata.
   - Shutdown Notification: `shutdown_notify` is an `Arc<Notify>` used to signal shutdown events to all running tasks.

2. **Asynchronous Task Spawning**
   - The `run` method of Messenger spawns multiple asynchronous tasks to handle blocks and transactions concurrently using Tokio’s `tokio::spawn`.

3. **Worker Tasks**
   - Two types of worker tasks are spawned: `transaction_worker` and `block_worker`.
     - Transaction Workers: Each worker waits for batches of transactions from the `transaction_receiver` channel and processes them by calling the `index_transaction` method on the DAO.
     - Block Workers: Each worker waits for batches of block metadata from the `block_receiver` channel and processes them by calling the `index_block_metadatas` method on the DAO.

4. **Handling Concurrency**
   - Shared State: The `transaction_receiver` and `block_receiver` are wrapped in `Arc<Mutex<...>>` to ensure safe concurrent access by multiple worker tasks.
   - Tokio Channels: Unbounded channels (`mpsc::unbounded_channel`) are used for communication between the main task and the worker tasks, supporting asynchronous, non-blocking operations.

5. **Error Handling and Retries**
   - Retries: If sending a block batch fails, the `send_block_batches` method will retry after a short delay to ensure robustness in case of transient errors.
   - Logging: Errors encountered during transaction and block processing are logged for monitoring and debugging purposes.

### Data Storage 

The requirement is to index transactions which, when inserted, are immutable. Given that the API mainly searches for transactions based on date, TimescaleDB is used. TimescaleDB provides features like:

- Extension on top of PostgreSQL
- Allows for hypertables and automatic partitioning
- Faster queries based on date
- Automatic deletion of older data, as we will not reasonably index all transactions from genesis

## API

There are 2 APIs currently supported:

`transactions/?id=<signature>`
`transactions/?day=dd/mm/yyyy`

The API part was rushed and I didn't get time to add more

## Integration Tests 

Tests are configured to run as "scenario" tests. They pull test input data from mainnet/devnet and store it locally to avoid tests breaking if mainnet/devnet data ever changes. The tests then feed the indexer functions and populate the indexed data in the database. Finally, an instance of the `Api` struct is created, queries are run against this struct, and the results are stored as snapshots through the `insta` testing library. Future runs of the same test are asserted to produce the same snapshot.

Note that tests do not actually run the indexer and API binaries; they only test the primary internal functions.

