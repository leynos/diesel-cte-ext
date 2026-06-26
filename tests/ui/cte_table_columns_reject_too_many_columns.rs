//! Protects the current `ColumnNames` limit by proving tables with more than
//! sixteen columns do not satisfy the type-level column-list contract.

diesel::table! {
    oversized (c01) {
        c01 -> diesel::sql_types::Integer,
        c02 -> diesel::sql_types::Integer,
        c03 -> diesel::sql_types::Integer,
        c04 -> diesel::sql_types::Integer,
        c05 -> diesel::sql_types::Integer,
        c06 -> diesel::sql_types::Integer,
        c07 -> diesel::sql_types::Integer,
        c08 -> diesel::sql_types::Integer,
        c09 -> diesel::sql_types::Integer,
        c10 -> diesel::sql_types::Integer,
        c11 -> diesel::sql_types::Integer,
        c12 -> diesel::sql_types::Integer,
        c13 -> diesel::sql_types::Integer,
        c14 -> diesel::sql_types::Integer,
        c15 -> diesel::sql_types::Integer,
        c16 -> diesel::sql_types::Integer,
        c17 -> diesel::sql_types::Integer,
    }
}

fn main() {
    let _columns = diesel_cte_ext::table_columns!(oversized::table);
}
