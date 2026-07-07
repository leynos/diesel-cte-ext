//! Compile-time checks for public macro and type-level contracts.

#![cfg(feature = "postgres")]

/// Verifies recursive search-order compile-time API contracts.
#[test]
fn recursive_search_api_compile_time_contracts() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/search_static_columns.rs");
    cases.compile_fail("tests/ui/search_non_static_column.rs");
    cases.compile_fail("tests/ui/search_config_is_private.rs");
}

/// Verifies CTE macro and type-level compile-time contracts.
#[test]
fn cte_macro_type_level_compile_time_contracts() {
    // CI runs this through the standard `make test` path. Refresh snapshots
    // after deliberately changing diagnostics with:
    // `TRYBUILD=overwrite cargo test --test trybuild --all-features`.
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/cte_dsl_recursive_parts_type_checks.rs");
    cases.compile_fail("tests/ui/cte_*reject*.rs");
}
