FROM rust:1.81-slim-bullseye
RUN apt update && apt install -y build-essential pkg-config libssl-dev

# Copy the project files
COPY . .

# Build the project
RUN cargo build --release -p indexer

RUN mv target/release/indexer itihasa-indexer

RUN rm -rf target