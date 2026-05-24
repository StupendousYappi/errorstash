# Project Overview

`errorstash` is a Rust library for collecting multiple related errors and reporting them together. This is particularly useful for scenarios like data validation, where you want to inform the user of all errors at once instead of failing on the first one.

The library is built around the `ErrorStash` trait, which provides a common interface for collecting errors. It offers two concrete implementations:

*   **`BoxedStash`**: A dynamically-typed stash that can collect errors of any type implementing `core::error::Error + Send + Sync + 'static`. It wraps them in a `BoxedErrorList`.
*   **`TypedStash`**: A statically-typed stash that collects errors of a specific type.

The library integrates with Rust's standard `Result` and `Iterator` types through the `StashableResult` and `StashErrorsIter` extension traits, respectively. This allows for idiomatic error handling using the `?` operator and methods like `or_stash` and `stash_errors`.

The project has no runtime dependencies, making it a lightweight addition to any Rust project.

# Building and Running

This is a library crate, so it's not meant to be run directly. However, you can build and test it using the standard Cargo commands:

*   **Build:**
    ```sh
    cargo build
    ```

*   **Run tests:**
    ```sh
    cargo test
    ```

# Development Conventions

*   **Error Handling**: The library is designed to be compatible with popular error handling crates like `anyhow`, `thiserror`, and `eyre`.
*   **Testing**: The project has a comprehensive test suite in each module. Tests are written using the standard `#[test]` attribute and can be run with `cargo test`. The `test-log` crate is used to enable logging during tests.
*   **Continuous Integration**: The project uses GitHub Actions for continuous integration. The CI pipeline runs audits, builds the project, and runs tests. The configuration can be found in `.github/workflows/`.
*   **Code Style**: The code follows standard Rust formatting and conventions.
