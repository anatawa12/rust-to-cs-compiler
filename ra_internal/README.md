# ra_internal: The only crate allowed to access rustc / rust analyzer internal api

As discussied in [Rust Analyzer Book], many crates like `ra_ap_hir_ty` are not API boundary, which means API is purely
unstable.
Only few crates like `ra_ap_syntax`, `ra_ap_hir` are API boundary and API are relatively stable.

To encapsulate API changes, we deny access to crates that are not API boundary from main crate and
allow access through this crate.

[Rust Analyzer Book]: https://rust-analyzer.github.io/book/contributing/architecture.html#code-map
