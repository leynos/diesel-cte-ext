//! Diesel extension crate providing support for recursive CTEs.
//!
//! The [`RecursiveCTEExt::with_recursive`] method builds a Diesel query
//! representing a `WITH RECURSIVE` block that can be executed like any other
//! query.

pub mod builders;
pub mod columns;
pub mod connection_ext;
pub mod cte;
pub mod macros;
#[cfg(test)]
pub(crate) mod test_support;

/// Bundles the CTE and body fragments handed to `with_cte`.
pub use builders::CteParts;
/// Bundles the seed, step, and body fragments handed to `with_recursive`.
pub use builders::RecursiveParts;
/// Builds a simple `WITH` block without the recursive union step.
pub use builders::with_cte;
#[doc = "Legacy helper kept for backwards compatibility with 0.1.0 previews."]
#[deprecated(note = "Use `RecursiveCTEExt::with_recursive` instead")]
pub use builders::with_recursive;
/// Runtime column names paired with compile-time schema metadata.
pub use columns::Columns;
/// Extension trait exposing the `with_recursive` helper on Diesel connections.
pub use connection_ext::RecursiveCTEExt;
/// Marker trait implemented by Diesel backends that can run recursive CTEs.
pub use cte::RecursiveBackend;
/// Wrapper for embedding Diesel fragments inside macro-driven queries.
pub use macros::QueryPart;
