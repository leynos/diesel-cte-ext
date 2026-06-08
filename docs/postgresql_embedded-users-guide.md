# postgresql_embedded user guide

The `postgresql_embedded` can download and manage embedded postgresql server for testing.
This allows better isolation and reproducibility of test environments and ease of testing
against different PostgreSQL versions.

## Prerequisites
- Rust toolchain specified in `rust-toolchain.toml`.
- Outbound network access to crates.io and the PostgreSQL binary archive.

## Quick start

1. Run the test suite, postgresql_embedded will download and install the latest
   PostgreSQL release and run the tests.
   ```bash
   cargo test
   ```

## Choose a specific version of PostgreSQL

1. Set the PG_VERSION_REQ environment variable to the version you want to use
   ```bash
   export PG_VERSION_REQ="16.4.0"
   ```
2. Run the test suite
   ```bash
   cargo test
   ```

## Use an existing PostgreSQL installation
1. Set the PG_VERSION_REQ to the version and PG_RUNTIME_DIR to the path of the installation
   ```bash
   # Example for macOS with the Postgres.app installed
   export PG_VERSION_REQ="15.0.0"
   export PG_RUNTIME_DIR="/Applications/Postgres.app/Contents/Versions/15/"
   ```
2. Run the test suite
   ```bash
   cargo test
   ```
