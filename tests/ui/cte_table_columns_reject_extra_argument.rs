//! Protects `table_columns!` arity by proving callers must provide exactly one
//! Diesel table path.

diesel::table! {
    sample (id) {
        id -> diesel::sql_types::Integer,
    }
}

/// Attempts to invoke `table_columns!` with more than one path.
fn main() {
    let _columns = diesel_cte_ext::table_columns!(sample::table, sample::id);
}
