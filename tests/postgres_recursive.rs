#![cfg(feature = "postgres")]
//! Behavioural tests for recursive CTE helpers on `PostgreSQL`.

use diesel::RunQueryDsl as DieselRunQueryDsl;
use diesel::{dsl::sql, sql_types::Bool, sql_types::Integer};
#[cfg(feature = "async")]
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl as AsyncRunQueryDsl};
use diesel_cte_ext::{CteParts, RecursiveCTEExt, RecursiveParts};
use pg_embedded_setup_unpriv::test_support::shared_cluster_handle;
use pg_embedded_setup_unpriv::{BootstrapResult, ClusterHandle, TemporaryDatabase};
use std::sync::atomic::{AtomicUsize, Ordering};

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

const TEMPLATE_DB_NAME: &str = "cte_ext_template";
static DB_COUNTER: AtomicUsize = AtomicUsize::new(0);

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

fn recursive_search_order_uses_postgres_search_clause(
    embedded_cluster: GuardedCluster,
    #[case] style: SearchStyle,
    #[case] expected: &[i32],
) -> TestResult<()> {
    let (_env_guard, cluster) = embedded_cluster?;
    let temp_db = templated_database(&cluster)?;
    let mut conn = cluster.connection().diesel_connection(temp_db.name())?;
    create_search_tree_fixture(&mut conn)?;

    let rows: Vec<i32> = DieselRunQueryDsl::load(
        conn.with_recursive_not_all(
            "tree",
            &["id", "parent_id"],
            RecursiveParts::new(
                sql::<Integer>("SELECT id, parent_id FROM search_nodes WHERE parent_id IS NULL"),
                sql::<Integer>(concat!(
                    "SELECT search_nodes.id, search_nodes.parent_id ",
                    "FROM search_nodes INNER JOIN tree ON search_nodes.parent_id = tree.id"
                )),
                sql::<Integer>("SELECT id FROM tree ORDER BY ordercol"),
            ),
        )
        .with_search(style, "id", "ordercol"),
        &mut conn,
    )?;

    if rows != expected {
        return Err(format!("expected {expected:?} but saw {rows:?}").into());
    }
    Ok(())
}

fn create_search_tree_fixture(conn: &mut diesel::pg::PgConnection) -> TestResult<()> {
    DieselRunQueryDsl::execute(
        sql_query(concat!(
            "CREATE TEMPORARY TABLE search_nodes (",
            "id INTEGER PRIMARY KEY, ",
            "parent_id INTEGER REFERENCES search_nodes(id)",
            ")"
        )),
        conn,
    )?;
    DieselRunQueryDsl::execute(
        sql_query(concat!(
            "INSERT INTO search_nodes (id, parent_id) VALUES ",
            "(1, NULL), ",
            "(2, 1), ",
            "(3, 1), ",
            "(4, 2), ",
            "(5, 2), ",
            "(6, 3)"
        )),
        conn,
    )?;
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
