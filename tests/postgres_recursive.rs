#![cfg(feature = "postgres")]
//! Behavioural tests for recursive CTE helpers on `PostgreSQL`.

use diesel::RunQueryDsl as DieselRunQueryDsl;
use diesel::{
    dsl::sql,
    sql_query,
    sql_types::{Bool, Integer, Nullable},
    allow_tables_to_appear_in_same_query, dsl::sql, prelude::*, sql_types::Bool,
    sql_types::Integer, table,
};
#[cfg(feature = "async")]
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl as AsyncRunQueryDsl};
use diesel_cte_ext::{CteParts, RecursiveCTEExt, RecursiveParts, SearchStyle};
use pg_embedded_setup_unpriv::test_support::shared_cluster_handle;
use pg_embedded_setup_unpriv::{BootstrapResult, ClusterHandle, TemporaryDatabase};
use proptest::{
    prelude::*,
    test_runner::{Config, TestCaseError},
};
use rstest::rstest;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::atomic::{AtomicUsize, Ordering},
};

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

const TEMPLATE_DB_NAME: &str = "cte_ext_template";
static DB_COUNTER: AtomicUsize = AtomicUsize::new(0);

const CREATE_CATEGORIES_TABLE: &str = "CREATE TABLE categories (
    id BIGINT PRIMARY KEY,
    parent_category_id BIGINT REFERENCES categories(id)
)";

const INSERT_CATEGORY_TREE: &str = "INSERT INTO categories (id, parent_category_id)
    VALUES (1, NULL), (2, 1), (3, 2), (4, 3)";
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

fn recursive_query_fragments_can_use_diesel_dsl() -> TestResult<()> {
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

    let cluster = shared_cluster_handle()?;
    let temp_db = templated_database(cluster)?;
    let mut conn = cluster.connection().diesel_connection(temp_db.name())?;
    DieselRunQueryDsl::execute(diesel::sql_query(CREATE_CATEGORIES_TABLE), &mut conn)?;
    DieselRunQueryDsl::execute(diesel::sql_query(INSERT_CATEGORY_TREE), &mut conn)?;

    let rows: Vec<i64> = DieselRunQueryDsl::load(
        conn.with_recursive_not_all(
            "parents",
            &["id"],
            RecursiveParts::new(
                categories::table
                    .select(categories::parent_category_id)
                    .filter(categories::id.eq(4_i64)),
                categories::table
                    .select(categories::parent_category_id)
                    .inner_join(
                        parents::table.on(parents::id.assume_not_null().eq(categories::id)),
                    ),
                parents::table
                    .select(parents::id.assume_not_null())
                    .filter(parents::id.is_not_null()),
            ),
        ),
        &mut conn,
    )?;

    let expected = [3_i64, 2, 1];
    if rows != expected {
        return Err(format!("expected {expected:?} but saw {rows:?}").into());
    }

    Ok(())
}

fn async_recursive_query_fragments_can_use_diesel_dsl() -> TestResult<()> {
    use tokio::runtime::Builder;

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
            conn.with_recursive_not_all(
                "parents",
                &["id"],
                RecursiveParts::new(
                    categories::table
                        .select(categories::parent_category_id)
                        .filter(categories::id.eq(4_i64)),
                    categories::table
                        .select(categories::parent_category_id)
                        .inner_join(
                            parents::table.on(parents::id.assume_not_null().eq(categories::id)),
                        ),
                    parents::table
                        .select(parents::id.assume_not_null())
                        .filter(parents::id.is_not_null()),
                ),
            ),
            &mut conn,
        )
        .await?;

        let expected = [3_i64, 2, 1];
        if rows != expected {
            return Err(format!("expected {expected:?} but saw {rows:?}").into());
        }

        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })?;

    Ok(())
}

#[rstest]
#[case::breadth_first(SearchStyle::BreadthFirst, &[1, 2, 3, 4, 5, 6])]
#[case::depth_first(SearchStyle::DepthFirst, &[1, 2, 4, 5, 3, 6])]
fn recursive_search_order_uses_postgres_search_clause(
    #[case] style: SearchStyle,
    #[case] expected: &[i32],
) -> TestResult<()> {
    let cluster = shared_cluster_handle()?;
    let temp_db = templated_database(cluster)?;
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
        .with_search(style, &["id", "parent_id"], "ordercol"),
        &mut conn,
    )?;

    if rows != expected {
        return Err(format!("expected {expected:?} but saw {rows:?}").into());
    }
    Ok(())
}

proptest! {
    #![proptest_config(Config {
        cases: 16,
        failure_persistence: None,
        ..Config::default()
    })]

    #[test]
    fn recursive_search_order_matches_generated_tree(tree in generated_search_tree()) {
        verify_generated_tree_order(&tree)
            .map_err(|err| TestCaseError::fail(err.to_string()))?;
    }
}

fn create_search_tree_fixture(conn: &mut diesel::pg::PgConnection) -> TestResult<()> {
    replace_search_tree_fixture(
        conn,
        &[
            (1, None),
            (2, Some(1)),
            (3, Some(1)),
            (4, Some(2)),
            (5, Some(2)),
            (6, Some(3)),
        ],
    )
}

fn generated_search_tree() -> impl Strategy<Value = Vec<(i32, Option<i32>)>> {
    (
        0usize..8,
        (
            0usize..1,
            0usize..2,
            0usize..3,
            0usize..4,
            0usize..5,
            0usize..6,
            0usize..7,
        ),
    )
        .prop_map(|(extra_nodes, generated_parent_indexes)| {
            let parent_indexes = [
                generated_parent_indexes.0,
                generated_parent_indexes.1,
                generated_parent_indexes.2,
                generated_parent_indexes.3,
                generated_parent_indexes.4,
                generated_parent_indexes.5,
                generated_parent_indexes.6,
            ];
            let child_ids = [2, 3, 4, 5, 6, 7, 8];
            let mut nodes = vec![(1, None)];
            let mut emitted_ids = vec![1];
            for (id, parent_index) in child_ids.into_iter().zip(parent_indexes).take(extra_nodes) {
                let parent_id = emitted_ids
                    .iter()
                    .copied()
                    .enumerate()
                    .find_map(|(index, node_id)| (index == parent_index).then_some(node_id))
                    .unwrap_or(1);
                nodes.push((id, Some(parent_id)));
                emitted_ids.push(id);
            }
            nodes
        })
}

fn verify_generated_tree_order(nodes: &[(i32, Option<i32>)]) -> TestResult<()> {
    let cluster = shared_cluster_handle()?;
    let temp_db = templated_database(cluster)?;
    let mut conn = cluster.connection().diesel_connection(temp_db.name())?;
    replace_search_tree_fixture(&mut conn, nodes)?;

    let breadth_first = load_search_order(&mut conn, SearchStyle::BreadthFirst)?;
    let expected_breadth_first = expected_breadth_first_order(nodes);
    if breadth_first != expected_breadth_first {
        return Err(format!(
            "expected breadth-first order {expected_breadth_first:?} but saw {breadth_first:?}"
        )
        .into());
    }

    let depth_first = load_search_order(&mut conn, SearchStyle::DepthFirst)?;
    let expected_depth_first = expected_depth_first_order(nodes);
    if depth_first != expected_depth_first {
        return Err(format!(
            "expected depth-first order {expected_depth_first:?} but saw {depth_first:?}"
        )
        .into());
    }

    Ok(())
}

fn replace_search_tree_fixture(
    conn: &mut diesel::pg::PgConnection,
    nodes: &[(i32, Option<i32>)],
) -> TestResult<()> {
    DieselRunQueryDsl::execute(
        sql_query(concat!(
            "CREATE TEMPORARY TABLE IF NOT EXISTS search_nodes (",
            "id INTEGER PRIMARY KEY, ",
            "parent_id INTEGER REFERENCES search_nodes(id)",
            ")"
        )),
        conn,
    )?;
    DieselRunQueryDsl::execute(sql_query("DELETE FROM search_nodes"), conn)?;

    for (id, parent_id) in nodes {
        DieselRunQueryDsl::execute(
            sql_query("INSERT INTO search_nodes (id, parent_id) VALUES ($1, $2)")
                .bind::<Integer, _>(*id)
                .bind::<Nullable<Integer>, _>(*parent_id),
            conn,
        )?;
    }
    Ok(())
}

fn load_search_order(
    conn: &mut diesel::pg::PgConnection,
    style: SearchStyle,
) -> TestResult<Vec<i32>> {
    let rows = DieselRunQueryDsl::load(
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
        .with_search(style, &["id", "parent_id"], "ordercol"),
        conn,
    )?;
    Ok(rows)
}

fn expected_breadth_first_order(nodes: &[(i32, Option<i32>)]) -> Vec<i32> {
    let mut depth_by_id = BTreeMap::from([(1, 0usize)]);
    let mut queue = VecDeque::from([1]);
    let children = children_by_parent(nodes);
    while let Some(id) = queue.pop_front() {
        let depth = depth_by_id.get(&id).copied().unwrap_or_default();
        if let Some(child_ids) = children.get(&id) {
            for child_id in child_ids {
                depth_by_id.insert(*child_id, depth + 1);
                queue.push_back(*child_id);
            }
        }
    }

    let mut ordered = nodes.to_vec();
    ordered.sort_by_key(|(id, parent_id)| {
        (
            depth_by_id.get(id).copied().unwrap_or_default(),
            *id,
            parent_id.unwrap_or_default(),
        )
    });
    ordered.into_iter().map(|(id, _parent_id)| id).collect()
}

fn expected_depth_first_order(nodes: &[(i32, Option<i32>)]) -> Vec<i32> {
    let children = children_by_parent(nodes);
    let mut order = Vec::new();
    push_depth_first_order(1, &children, &mut order);
    order
}

fn push_depth_first_order(id: i32, children: &BTreeMap<i32, Vec<i32>>, order: &mut Vec<i32>) {
    order.push(id);
    if let Some(child_ids) = children.get(&id) {
        for child_id in child_ids {
            push_depth_first_order(*child_id, children, order);
        }
    }
}

fn children_by_parent(nodes: &[(i32, Option<i32>)]) -> BTreeMap<i32, Vec<i32>> {
    let mut children = BTreeMap::new();
    for (id, maybe_parent_id) in nodes {
        if let Some(parent_id) = maybe_parent_id {
            children
                .entry(*parent_id)
                .or_insert_with(Vec::new)
                .push(*id);
        }
    }
    children
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
