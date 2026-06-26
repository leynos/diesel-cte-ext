# User guide

This guide explains how to compose Common Table Expressions (CTEs) with
`diesel-cte-ext`, configure features, and exercise the crate inside automated
suites. Follow the documentation style guide in
`docs/documentation-style-guide.md` when extending this file.

## Overview

`diesel-cte-ext` adds two ergonomic layers on top of Diesel:

- the `RecursiveCTEExt` trait, which exposes `with_cte` and `with_recursive`
  constructors on synchronous and async connection types, plus
  `with_recursive_not_all` for recursive `UNION` behaviour;
- the `Columns` utilities that keep runtime column names aligned with Diesel's
  compile-time type metadata.

The crate works with SQLite and PostgreSQL backends out of the box. Enable the
`async` feature when you need `diesel_async` connections.

## Feature flags

| Feature    | Purpose                                          |
| ---------- | ------------------------------------------------ |
| `sqlite`   | Enables Diesel's SQLite backend integration.     |
| `postgres` | Enables Diesel's PostgreSQL backend integration. |
| `async`    | Adds `diesel_async` support for both backends.   |

All examples in this document assume the default feature set (`sqlite` +
`postgres`). Enable `async` when compiling the async snippets or running the
integration tests.

## Building non-recursive CTEs

Use `with_cte` to create a single `WITH` block without a recursive step. Bundle
the CTE body and the consuming query using `CteParts::new` before passing them
to the helper.

```rust,no_run
use diesel::{dsl::sql, sqlite::SqliteConnection, sql_types::Text, RunQueryDsl};
use diesel_cte_ext::{CteParts, RecursiveCTEExt};

fn names() -> diesel::QueryResult<Vec<String>> {
    let mut conn = SqliteConnection::establish(":memory:")?;
    conn.with_cte(
        "names",
        &["label"],
        CteParts::new(
            sql::<Text>("SELECT 'root' AS label UNION ALL SELECT 'child'"),
            sql::<Text>("SELECT label FROM names ORDER BY label"),
        ),
    )
    .load(&mut conn)
}
```

## Building recursive CTEs

Recursive queries delegate the three constituent fragments (seed, recursive
step, and final body) to a `RecursiveParts` struct. Each fragment can be a
normal Diesel query builder expression, so Diesel validates the AST at compile
time instead of leaving the CTE body as raw SQL.

```rust,no_run
use diesel::{allow_tables_to_appear_in_same_query, pg::PgConnection, prelude::*, table};
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts};

fn parent_category_ids(
    conn: &mut PgConnection,
    category_id: i64,
) -> diesel::QueryResult<Vec<i64>> {
    table! {
        categories (id) {
            id -> BigInt,
            parent_category_id -> Nullable<BigInt>,
        }
    }

    table! {
        parents (id) {
            id -> Nullable<BigInt>,
        }
    }

    allow_tables_to_appear_in_same_query!(categories, parents);

    conn.with_recursive_not_all(
        "parents",
        &["id"],
        RecursiveParts::new(
            categories::table
                .select(categories::parent_category_id)
                .filter(categories::id.eq(category_id)),
            categories::table
                .select(categories::parent_category_id)
                .inner_join(
                    parents::table.on(parents::id.assume_not_null().eq(categories::id)),
                ),
            parents::table
                .select(parents::id.assume_not_null())
                .filter(parents::id.is_not_null()),
        ),
    )
    .load(conn)
}
```

`with_recursive` renders a recursive `UNION ALL`. Use `with_recursive_not_all`
when the recursive term should deduplicate at each iteration using `UNION`, as
shown above.

Async connections receive the same helpers once the `async` feature is enabled:

```rust,no_run
use diesel::{allow_tables_to_appear_in_same_query, prelude::*, table};
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts};

async fn parent_category_ids_async(
    conn: &mut AsyncPgConnection,
    category_id: i64,
) -> diesel::QueryResult<Vec<i64>> {
    table! {
        categories (id) {
            id -> BigInt,
            parent_category_id -> Nullable<BigInt>,
        }
    }

    table! {
        parents (id) {
            id -> Nullable<BigInt>,
        }
    }

    allow_tables_to_appear_in_same_query!(categories, parents);

    conn.with_recursive_not_all(
        "parents",
        &["id"],
        RecursiveParts::new(
            categories::table
                .select(categories::parent_category_id)
                .filter(categories::id.eq(category_id)),
            categories::table
                .select(categories::parent_category_id)
                .inner_join(
                    parents::table.on(parents::id.assume_not_null().eq(categories::id)),
                ),
            parents::table
                .select(parents::id.assume_not_null())
                .filter(parents::id.is_not_null()),
        ),
    )
    .load(conn)
    .await
}
```

## Column helpers

Manual column lists are easy to mistype, especially when a recursive step spans
multiple tables. The `columns!` macro accepts individual column paths and emits
both the runtime names and the tuple of Diesel column types. Use
`table_columns!` to refer to a Diesel table definition and capture every column
in declaration order.

```rust
use diesel::{prelude::*, sql_types::Integer};
use diesel_cte_ext::{columns, table_columns, Columns};

diesel::table! {
    employees (id) {
        id -> Integer,
        manager_id -> Integer,
    }
}

const MANAGER_COLUMNS: Columns<(employees::id, employees::manager_id)> =
    columns!(employees::id, employees::manager_id);
const FULL_TABLE: Columns<employees::table> = table_columns!(employees::table);
```

## Macro helpers for inline fragments

Use `cte_query!`, `seed_query!`, and `step_query!` to wrap ad-hoc Diesel
expressions before passing them into `RecursiveParts::new`. The macros keep the
fragments strongly typed whilst avoiding manual `QueryPart` construction and
make the exported helpers more visible when developers scan the module surface.

```rust,no_run
use diesel::{dsl::sql, sql_types::Integer};
use diesel_cte_ext::{RecursiveParts, cte_query, seed_query, step_query};

let parts = RecursiveParts::new(
    seed_query!(sql::<Integer>("SELECT 1")),
    step_query!(sql::<Integer>("SELECT n + 1 FROM series")),
    cte_query!(sql::<Integer>("SELECT n FROM series")),
);
```

## Testing with `pg_embedded_setup_unpriv`

The PostgreSQL integration tests use `pg_embedded_setup_unpriv` to run embedded
PostgreSQL without requiring a system PostgreSQL service. A test process starts
one shared embedded cluster handle, and each PostgreSQL test receives its own
temporary database cloned from a template. This keeps database state isolated
without paying the cost of starting a new server for every test.

Use `make test` for the normal workflow. The Makefile runs `prepare-pg-worker`,
builds the locked `pg_worker` helper, exports `PG_EMBEDDED_WORKER`, and creates
a unique writable runtime base under `target/pg-embed-runs/<run-id>`.
`PG_RUNTIME_DIR` and `PG_DATA_DIR` live under that base, which is created with
the sticky bit set, so the helper can use it safely.

When bypassing `make test`, unprivileged `cargo test` runs may use the upstream
defaults. Root-capable runs must prepare `pg_worker` first and export
`PG_EMBEDDED_WORKER` themselves; run `make prepare-pg-worker`, then set
`PG_EMBEDDED_WORKER=target/pg_worker`.

The suite deliberately does not use an external PostgreSQL test URL. Embedded
PostgreSQL keeps local, Continuous Integration, and sandboxed agent runs on the
same lifecycle and authentication path, which makes failures easier to
reproduce across environments.
