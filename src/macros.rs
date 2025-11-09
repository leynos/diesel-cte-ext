//! Macros and helpers for embedding Diesel expressions inside recursive CTEs.
use diesel::{
    backend::Backend,
    query_builder::{AstPass, QueryFragment, QueryId},
    result::QueryResult,
};

/// Wrapper for Diesel expressions used in recursive CTEs.
#[derive(Debug, Clone)]
pub struct QueryPart<T>(pub T);

impl<T> QueryPart<T> {
    /// Wrap a Diesel expression for use with `with_recursive`.
    pub const fn new(expr: T) -> Self {
        Self(expr)
    }
}

impl<T> From<T> for QueryPart<T> {
    fn from(expr: T) -> Self {
        Self(expr)
    }
}

impl<DB, T> QueryFragment<DB> for QueryPart<T>
where
    DB: Backend,
    T: QueryFragment<DB>,
{
    fn walk_ast<'b>(&'b self, out: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        self.0.walk_ast(out)
    }
}

impl<T> QueryId for QueryPart<T>
where
    T: QueryId,
{
    type QueryId = T::QueryId;
    const HAS_STATIC_QUERY_ID: bool = T::HAS_STATIC_QUERY_ID;
}

#[macro_export]
/// Wrap a Diesel expression for use inside a recursive CTE.
macro_rules! cte_query {
    ($expr:expr $(,)?) => {
        $crate::QueryPart::new($expr)
    };
}

#[macro_export]
#[doc = "Wrap a Diesel expression to use as the seed query in a recursive CTE."]
#[doc = ""]
#[doc = "# Example"]
#[doc = ""]
#[doc = "```"]
#[doc = "use diesel::dsl::sql;"]
#[doc = "use diesel::sql_types::Integer;"]
#[doc = "use diesel_cte_ext::seed_query;"]
#[doc = ""]
#[doc = "let part = seed_query!(sql::<Integer>(\"SELECT 1\"));"]
#[doc = "```"]
macro_rules! seed_query {
    ($expr:expr $(,)?) => {
        $crate::cte_query!($expr)
    };
}

#[macro_export]
#[doc = "Wrap a Diesel expression to use as a step query within a recursive CTE."]
#[doc = ""]
#[doc = "# Example"]
#[doc = ""]
#[doc = "```"]
#[doc = "use diesel::dsl::sql;"]
#[doc = "use diesel::sql_types::Integer;"]
#[doc = "use diesel_cte_ext::step_query;"]
#[doc = ""]
#[doc = "let part = step_query!(sql::<Integer>(\"SELECT n + 1 FROM t\"));"]
#[doc = "```"]
macro_rules! step_query {
    ($expr:expr $(,)?) => {
        $crate::cte_query!($expr)
    };
}

#[cfg(test)]
mod tests {
    use super::QueryPart;
    use crate::test_support::normalise_debug_sql;
    use diesel::{debug_query, dsl::sql, sql_types::Integer, sqlite::Sqlite};

    #[test]
    fn seed_query_wraps_expression() {
        let literal = sql::<Integer>("SELECT 1");
        let wrapped = seed_query!(literal);
        assert_sql_matches(&wrapped, "SELECT 1");
    }

    #[test]
    fn step_query_wraps_expression() {
        let literal = sql::<Integer>("SELECT n + 1 FROM t");
        let wrapped = step_query!(literal);
        assert_sql_matches(&wrapped, "SELECT n + 1 FROM t");
    }

    #[test]
    fn cte_query_wraps_expression() {
        let literal = sql::<Integer>("SELECT 42");
        let wrapped = cte_query!(literal);
        assert_sql_matches(&wrapped, "SELECT 42");
    }

    fn assert_sql_matches<T>(part: &QueryPart<T>, expected: &str)
    where
        T: diesel::query_builder::QueryFragment<Sqlite>,
    {
        let rendered = normalise_debug_sql(&debug_query::<Sqlite, _>(part).to_string());
        assert_eq!(rendered, expected);
    }
}
