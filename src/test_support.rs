//! Internal helpers dedicated to unit and integration tests.

/// Normalise Diesel's `debug_query` output for string comparisons.
///
/// Diesel's `SQLite` backend emits identifiers wrapped in backticks and appends
/// ` -- binds: [...]` to the rendered SQL. This helper trims trailing
/// whitespace, strips the bind suffix, and replaces the SQLite-specific
/// backticks with ANSI double quotes so tests can perform straightforward
/// assertions regardless of backend quirks.
#[must_use]
pub(crate) fn normalise_debug_sql(sql: &str) -> String {
    let trimmed = sql.trim();
    let without_binds = trimmed
        .split_once(" -- binds: ")
        .map_or(trimmed, |(statement, _)| statement)
        .trim_end();
    without_binds.replace('`', "\"")
}
