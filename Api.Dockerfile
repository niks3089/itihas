FROM rust:1.81-slim-bullseye
RUN apt update && apt install -y build-essential pkg-config libssl-dev

# Copy the project files
COPY . .

# Build the project
RUN cargo build --release -p api

RUN mv target/release/api itihasa-api

RUN rm -rf target