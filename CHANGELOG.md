# Changelog

## [0.1.2] - 2026-07-21

### Changed

- Bump `pg-embed-setup-unpriv` from 0.5.1 to 0.5.2, raising the pinned Rust
  toolchain to 1.94.0 to satisfy the new transitive minimum supported Rust
  version
  ([`97ad776`](https://github.com/leynos/diesel-cte-ext/commit/97ad776573250b8e28f81d28939f368368ee33b0)).

## [0.1.1] - 2026-06-25

### Changed

- Document the `pg-embed-setup-unpriv` 0.5.1 test dependency introduced by
  [`302d156`](https://github.com/leynos/diesel-cte-ext/commit/302d156361161fd73310926dcef6513b41f7b393),
  including root-safe embedded PostgreSQL setup guidance for sandboxed agentic
  development.

[0.1.2]: https://github.com/leynos/diesel-cte-ext/commit/97ad776573250b8e28f81d28939f368368ee33b0
[0.1.1]: https://github.com/leynos/diesel-cte-ext/commit/302d156361161fd73310926dcef6513b41f7b393
