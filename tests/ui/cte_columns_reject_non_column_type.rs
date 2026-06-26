//! Protects `columns!` by proving each path must name a Diesel column type,
//! not an arbitrary Rust type.

struct PlainType;

fn main() {
    let _columns = diesel_cte_ext::columns!(PlainType);
}
