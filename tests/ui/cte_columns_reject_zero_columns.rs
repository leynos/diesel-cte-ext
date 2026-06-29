//! Protects `columns!` arity by proving callers must provide at least one
//! Diesel column path.

/// Attempts to invoke `columns!` without any column paths.
fn main() {
    let _columns = diesel_cte_ext::columns!();
}
