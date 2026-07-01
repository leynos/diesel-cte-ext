#![cfg(feature = "postgres")]
//! Behavioural tests for recursive CTE helpers on `PostgreSQL`.

use diesel::RunQueryDsl as DieselRunQueryDsl;
use diesel::{
    allow_tables_to_appear_in_same_query, dsl::sql, prelude::*, sql_types::Bool,
    sql_types::Integer, table,
};
#[cfg(feature = "async")]
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl as AsyncRunQueryDsl};
use diesel_cte_ext::{CteParts, RecursiveCTEExt, RecursiveParts};
use pg_embedded_setup_unpriv::test_support::shared_cluster_handle;
use pg_embedded_setup_unpriv::{BootstrapResult, ClusterHandle, TemporaryDatabase};
use std::sync::atomic::{AtomicUsize, Ordering};

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

const TEMPLATE_DB_NAME: &str = "cte_ext_template";
static DB_COUNTER: AtomicUsize = AtomicUsize::new(0);
const CREATE_CATEGORIES_TABLE: &str = "CREATE TABLE categories (
    id BIGINT PRIMARY KEY,
    parent_category_id BIGINT REFERENCES categories(id)
)";
const INSERT_CATEGORY_TREE: &str = "INSERT INTO categories (id, parent_category_id)
    VALUES (1, NULL), (2, 1), (3, 2), (4, 3)";

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

/// Builds the seed, step and body fragments for the "ancestor categories"
/// recursive query documented in `docs/users-guide.md`, shared by the sync
/// and async Diesel DSL recursive query tests.
macro_rules! ancestor_category_parts {
    ($category_id:expr) => {
        RecursiveParts::new(
            categories::table
                .select(categories::parent_category_id)
                .filter(categories::id.eq($category_id)),
            categories::table
                .select(categories::parent_category_id)
                .inner_join(
                    parents::table.on(parents::id.assume_not_null().eq(categories::id)),
                ),
            parents::table
                .select(parents::id.assume_not_null())
                .filter(parents::id.is_not_null()),
        )
    };
}

/// Asserts that `rows` match the ancestor chain expected for category `4`
/// in the fixture seeded by [`INSERT_CATEGORY_TREE`].
fn ensure_ancestor_chain(rows: &[i64]) -> TestResult<()> {
    let expected = [3_i64, 2, 1];
    if rows != expected {
        return Err(format!("expected {expected:?} but saw {rows:?}").into());
    }
    Ok(())
}

fn next_database_name() -> String {
    let id = DB_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("cte_ext_test_{id}")
}

fn templated_database(cluster: &ClusterHandle) -> BootstrapResult<TemporaryDatabase> {
    cluster.ensure_template_exists(TEMPLATE_DB_NAME, |_db_name| Ok(()))?;
    cluster.temporary_database_from_template(next_database_name(), TEMPLATE_DB_NAME)
}

#[test]
fn recursive_sequence_via_sync_conn() -> TestResult<()> {
    let cluster = shared_cluster_handle()?;
    let temp_db = templated_database(cluster)?;
    let mut conn = cluster.connection().diesel_connection(temp_db.name())?;

    let rows: Vec<i32> = DieselRunQueryDsl::load(
        conn.with_recursive(
            "t",
            &["n"],
            RecursiveParts::new(
                sql::<Integer>("SELECT 1"),
                sql::<Integer>("SELECT n + 1 FROM t WHERE n < 5"),
                sql::<Integer>("SELECT n FROM t ORDER BY n"),
            ),
        ),
        &mut conn,
    )?;

    let expected = [1, 2, 3, 4, 5];
    if rows != expected {
        return Err(format!("expected {expected:?} but saw {rows:?}").into());
    }
    Ok(())
}

#[cfg(feature = "async")]
#[test]
fn recursive_sequence_via_async_conn() -> TestResult<()> {
    use tokio::runtime::Builder;

    let cluster = shared_cluster_handle()?;
    let temp_db = templated_database(cluster)?;
    let rt = Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime");

    let db_url = temp_db.url().to_owned();

    rt.block_on(async move {
        let mut conn = AsyncPgConnection::establish(&db_url).await?;

        let rows: Vec<i32> = AsyncRunQueryDsl::load(
            conn.with_recursive(
                "t",
                &["n"],
                RecursiveParts::new(
                    sql::<Integer>("SELECT 1"),
                    sql::<Integer>("SELECT n + 1 FROM t WHERE n < 5"),
                    sql::<Integer>("SELECT n FROM t ORDER BY n"),
                ),
            ),
            &mut conn,
        )
        .await?;

        let expected = [1, 2, 3, 4, 5];
        if rows != expected {
            return Err(format!("expected {expected:?} but saw {rows:?}").into());
        }

        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })?;

    Ok(())
}

#[test]
fn non_recursive_cte_returns_seed() -> TestResult<()> {
    let cluster = shared_cluster_handle()?;
    let temp_db = templated_database(cluster)?;
    let mut conn = cluster.connection().diesel_connection(temp_db.name())?;

    let result: i32 = DieselRunQueryDsl::get_result(
        conn.with_cte(
            "seed",
            &["value"],
            CteParts::new(
                sql::<Integer>("SELECT 42"),
                sql::<Integer>("SELECT value FROM seed"),
            ),
        ),
        &mut conn,
    )?;

    if result != 42 {
        return Err("seed CTE did not round-trip 42".into());
    }
    Ok(())
}

#[test]
fn recursive_query_fragments_can_use_diesel_dsl() -> TestResult<()> {
    let cluster = shared_cluster_handle()?;
    let temp_db = templated_database(cluster)?;
    let mut conn = cluster.connection().diesel_connection(temp_db.name())?;
    DieselRunQueryDsl::execute(diesel::sql_query(CREATE_CATEGORIES_TABLE), &mut conn)?;
    DieselRunQueryDsl::execute(diesel::sql_query(INSERT_CATEGORY_TREE), &mut conn)?;

    let rows: Vec<i64> = DieselRunQueryDsl::load(
        conn.with_recursive_not_all("parents", &["id"], ancestor_category_parts!(4_i64)),
        &mut conn,
    )?;

    ensure_ancestor_chain(&rows)
}

#[cfg(feature = "async")]
#[test]
fn async_recursive_query_fragments_can_use_diesel_dsl() -> TestResult<()> {
    use tokio::runtime::Builder;

    let cluster = shared_cluster_handle()?;
    let temp_db = templated_database(cluster)?;
    let db_url = temp_db.url().to_owned();
    let rt = Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime");

    rt.block_on(async move {
        let mut conn = AsyncPgConnection::establish(&db_url).await?;
        AsyncRunQueryDsl::execute(diesel::sql_query(CREATE_CATEGORIES_TABLE), &mut conn).await?;
        AsyncRunQueryDsl::execute(diesel::sql_query(INSERT_CATEGORY_TREE), &mut conn).await?;

        let rows: Vec<i64> = AsyncRunQueryDsl::load(
            conn.with_recursive_not_all("parents", &["id"], ancestor_category_parts!(4_i64)),
            &mut conn,
        )
        .await?;

        ensure_ancestor_chain(&rows)
    })?;

    Ok(())
}

#[test]
fn templated_databases_isolate_state_on_shared_cluster() -> TestResult<()> {
    let cluster = shared_cluster_handle()?;
    let first_db = templated_database(cluster)?;
    let second_db = templated_database(cluster)?;

    let mut first_conn = cluster.connection().diesel_connection(first_db.name())?;
    DieselRunQueryDsl::execute(
        diesel::sql_query("CREATE TABLE isolation_marker (id INTEGER PRIMARY KEY)"),
        &mut first_conn,
    )?;

    let mut second_conn = cluster.connection().diesel_connection(second_db.name())?;
    let is_missing: bool = DieselRunQueryDsl::get_result(
        diesel::select(sql::<Bool>(
            "to_regclass('public.isolation_marker') IS NULL",
        )),
        &mut second_conn,
    )?;

    if !is_missing {
        return Err("template-cloned temporary databases leaked table state".into());
    }

    Ok(())
}
