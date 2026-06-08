#![cfg(feature = "postgres")]
//! Behavioural tests for recursive CTE helpers on `PostgreSQL`.

use diesel::{PgConnection, RunQueryDsl as DieselRunQueryDsl};
use diesel::{dsl::sql, sql_types::Integer};
#[cfg(feature = "async")]
use diesel_async::{AsyncPgConnection, RunQueryDsl as AsyncRunQueryDsl};
use diesel_cte_ext::{CteParts, RecursiveCTEExt, RecursiveParts};
use rstest::{fixture, rstest};

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

async fn embedded_server_url() -> Result<String, String> {
    use postgresql_embedded as pge;

    static TEST_DATABASE_NAME: &str = "cte_ext_template";

    static GLOBAL_RESOURCE: tokio::sync::OnceCell<pge::Result<pge::PostgreSQL>> =
        tokio::sync::OnceCell::const_new();

    let server = GLOBAL_RESOURCE
        .get_or_init(|| async {
            let mut settings = pge::Settings::default();

            if let Ok(ver) = std::env::var("PG_VERSION_REQ") {
                settings.version = pge::VersionReq::parse(&ver)?;
            }

            if let Ok(runtime) = std::env::var("PG_RUNTIME_DIR") {
                settings.installation_dir = runtime.into();
                settings.trust_installation_dir = true;
            }

            let mut pg = pge::PostgreSQL::new(settings);

            pg.setup().await?;
            pg.start().await?;

            pg.create_database(TEST_DATABASE_NAME).await?;

            Ok(pg)
        }).await.as_ref().map_err(ToString::to_string)?;

    Ok(server.settings().url(TEST_DATABASE_NAME))
}

/// Returns a sync connection to the embedded database.
#[fixture]
pub async fn test_pg_connection() -> PgConnection {
    use diesel::Connection;
    let connection_url = embedded_server_url().await.expect("Could not start embedded database");

    PgConnection::establish(&connection_url).expect("Could not connect to embedded database")
}

/// returns an async connection to the embedded database.
#[cfg(feature = "async")]
#[fixture]
pub async fn test_async_pg_connection() -> AsyncPgConnection {
    use diesel_async::AsyncConnection;

    let connection_url = embedded_server_url().await.expect("Could not start embedded database");

    AsyncPgConnection::establish(&connection_url).await.expect("Could not connect to embedded database")
}

#[rstest]
#[tokio::test]
async fn recursive_sequence_via_sync_conn(#[future] test_pg_connection: PgConnection) -> TestResult<()> {
    let mut conn = test_pg_connection.await;

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
#[rstest]
#[tokio::test]
async fn recursive_sequence_via_async_conn(#[future] test_async_pg_connection: AsyncPgConnection) -> TestResult<()> {
    let mut conn = test_async_pg_connection.await;

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

    Ok(())
}

#[rstest]
#[tokio::test]
async fn non_recursive_cte_returns_seed(#[future] test_pg_connection: PgConnection) -> TestResult<()> {
    let mut conn = test_pg_connection.await;

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
