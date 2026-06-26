//! Protects `columns!` arity by proving callers must provide at least one
//! Diesel column path.

fn main() {
    let _columns = diesel_cte_ext::columns!();
}
