# diesel-cte-ext

A cheerful toolkit for building recursive and non-recursive Common Table
Expressions (CTEs) on top of Diesel. The helpers wrap the boilerplate needed to
assemble `WITH` blocks, keep column metadata tidy, and work with both blocking
and async Diesel connections.

## Highlights

- Friendly builders for `WITH` and `WITH RECURSIVE` queries via
  `RecursiveCTEExt`.
- Column helpers (`columns!` and `table_columns!`) that marry runtime names to
  compile-time Diesel metadata.
- Async-ready: enable the `async` feature to extend the helpers to
  `diesel_async` connections.
- Battle-tested Postgres integration tests powered by
  `pg_embedded_setup_unpriv`, so CI agents without root privileges can still
  run the suite end-to-end.

## Quick start

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
diesel-cte-ext = { version = "0.1", default-features = false, features = [
    "postgres",
] }
```

Build a recursive query:

```rust,no_run
use diesel::{dsl::sql, pg::PgConnection, sql_types::Integer, RunQueryDsl};
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts};

fn five_high(mut conn: PgConnection) -> diesel::QueryResult<Vec<i32>> {
    conn.with_recursive(
        "series",
        &["n"],
        RecursiveParts::new(
            sql::<Integer>("SELECT 1"),
            sql::<Integer>("SELECT n + 1 FROM series WHERE n < 5"),
            sql::<Integer>("SELECT n FROM series"),
        ),
    )
    .load(&mut conn)
}
```

Wrap Diesel expressions for the seed, step, or body fragments using the macro
helpers when you need to inline SQL snippets:

```rust,no_run
use diesel::{dsl::sql, sql_types::Integer};
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts, cte_query, seed_query, step_query};

let parts = RecursiveParts::new(
    seed_query!(sql::<Integer>("SELECT 1")),
    step_query!(sql::<Integer>("SELECT n + 1 FROM series")),
    cte_query!(sql::<Integer>("SELECT n FROM series")),
);
```

See `docs/users-guide.md` for a fuller tour, including trait diagrams and
advanced builder patterns.

## Examples

Run the ready-to-go examples after enabling the relevant backend feature:

```bash
cargo run --example sqlite_fibonacci
cargo run --example sqlite_directory
```

Both examples use in-memory SQLite so they are quick to run repeatedly.

## Testing

This repository ships a `Makefile` that drives formatting, linting, and tests
with the strict flags configured in `Cargo.toml`:

```bash
make check-fmt
make lint
make test
```

`make test` spins up an embedded PostgreSQL instance via
`pg_embedded_setup_unpriv`. The helper automatically provisions binaries,
configures `$PGPASSFILE`, and shuts everything down between tests. No manual
Postgres installation is required.

Have fun composing queries!
