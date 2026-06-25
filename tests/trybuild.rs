//! Compile-time checks for the public recursive search-order API.

#[test]
fn recursive_search_api_compile_time_contracts() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/search_static_columns.rs");
    cases.compile_fail("tests/ui/search_non_static_column.rs");
    cases.compile_fail("tests/ui/search_config_is_private.rs");
}
