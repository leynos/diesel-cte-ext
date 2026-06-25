//! Search-order support types for recursive CTE queries.

use diesel::backend::Backend;

/// Column list used by a recursive CTE `SEARCH ... BY` clause.
#[derive(Debug, Clone, Copy)]
pub struct SearchColumnList {
    names: SearchColumnNames,
}

impl SearchColumnList {
    /// Return the search column names in clause order.
    #[must_use]
    pub(crate) const fn names(self) -> SearchColumnNames {
        self.names
    }
}

impl From<&'static str> for SearchColumnList {
    fn from(name: &'static str) -> Self {
        Self {
            names: SearchColumnNames::Single(name),
        }
    }
}

impl From<&'static [&'static str]> for SearchColumnList {
    fn from(names: &'static [&'static str]) -> Self {
        Self {
            names: SearchColumnNames::List(names),
        }
    }
}

impl<const N: usize> From<&'static [&'static str; N]> for SearchColumnList {
    fn from(names: &'static [&'static str; N]) -> Self {
        Self::from(&names[..])
    }
}

/// Runtime storage for one or more recursive CTE search columns.
#[derive(Debug, Clone, Copy)]
pub(crate) enum SearchColumnNames {
    Single(&'static str),
    List(&'static [&'static str]),
}

/// Internal search clause configuration for recursive CTEs.
#[derive(Debug, Clone)]
pub(crate) struct SearchConfig {
    pub(crate) style: SearchStyle,
    pub(crate) search_columns: SearchColumnList,
    pub(crate) output_column: &'static str,
}

/// Define the search mode to tell the DB to use when scanning the recursive CTE.
#[derive(Debug, Clone, Copy)]
pub enum SearchStyle {
    /// Tells the DB to perform a breadth first scan of the recursive CTE.
    BreadthFirst,
    /// Tells the DB to perform a depth first scan of the recursive CTE.
    DepthFirst,
}

impl SearchStyle {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::BreadthFirst => "BREADTH FIRST",
            Self::DepthFirst => "DEPTH FIRST",
        }
    }
}

#[cfg(feature = "postgres")]
pub(crate) fn supports_search_clause<DB: Backend>() -> bool {
    std::any::type_name::<DB>() == std::any::type_name::<diesel::pg::Pg>()
}

#[cfg(not(feature = "postgres"))]
pub(crate) const fn supports_search_clause<DB: Backend>() -> bool {
    false
}
