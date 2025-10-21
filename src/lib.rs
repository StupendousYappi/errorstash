//! Utilities for collecting multiple errors and emitting them as a single wrapper error.
//!
//! Error stashes are useful when you want to perform multiple operations that
//! may independently fail, and you want to collect all errors that occur and
//! return them together, rather than failing fast on the first error. For
//! example, when validating data, you may want to provide the caller with a
//! complete list of validation errors so that they can fix them all at once.
//!
//! ## Example
//! ```
//! use std::collections::HashSet;
//! use url::Url;
//! use thiserror::Error;
//! use errorstash::{ErrorStash, BoxedStash, BoxedErrors, StashableResult};
//!
//! #[derive(Error, Debug)]
//! #[error("Domain '{0}' is not trusted")]
//! struct InvalidDomainError(String);
//!
//! #[derive(Debug, PartialEq, Eq)]
//! struct TrustedUrl(Url);
//!
//! #[derive(Debug, Clone, PartialEq, Eq)]
//! struct UrlValidator(HashSet<String>);
//!
//! impl UrlValidator {
//!     fn new(trusted_domains: Vec<String>) -> Self {
//!         Self(trusted_domains.into_iter().collect())
//!     }
//!
//!     /// Validates that the given URL complies with our requirements, and if
//!     /// so, returns it as a `TrustedUrl`.
//!     fn validate_url(&self, url: &str) -> Result<TrustedUrl, BoxedErrors> {
//!         let mut stash = BoxedStash::with_summary("Invalid URL:");
//!         let parsed = Url::parse(url).or_fail(&mut stash)?;
//!
//!         stash
//!             .check(parsed.scheme() == "https", "URL must use https scheme")
//!             .check(parsed.port().is_none(), "URL must not specify a port");
//!         let raw_domain = parsed.host_str().unwrap_or_default();
//!         if raw_domain.is_empty() {
//!             stash.push("URL must have a domain");
//!         } else if !self.0.contains(raw_domain) {
//!             let e = InvalidDomainError(raw_domain.into());
//!             stash.push(e);
//!         }
//!         stash.fail_unless_empty()?;
//!
//!         let as_safe = TrustedUrl(parsed);
//!         Ok(as_safe)
//!     }
//! }
//!
//! let validator = UrlValidator::new(vec!["example.com".to_string(), "rust-lang.org".to_string()]);
//!
//! let good = validator.validate_url("https://example.com/path").unwrap();
//! assert_eq!("https://example.com/path", good.0.as_str());
//!
//! let bad = validator.validate_url("http://untrusted.com:8080/path");
//! assert!(bad.is_err());
//! let errors = bad.unwrap_err();
//! assert_eq!(3, errors.len());
//! let expected = "
//! Invalid URL:
//! - URL must use https scheme
//! - URL must not specify a port
//! - Domain 'untrusted.com' is not trusted
//! ";
//! assert_eq!(expected.trim(), errors.to_string().trim());
//! ```
//!
//! # Key features
//!
//! - Compatible with any error that implements [`std::error::Error`], including
//!   errors created by [`anyhow`], [`thiserror`], [`eyre`].
//! - Works with the `?` operator and standard `Result` types
//!   - Supports easily adding errors from a `Result` via the [`or_stash`][StashableResult::or_stash]
//!     and [`or_fail`][StashableResult::or_fail] extension trait methods.
//! - Supports easily adding errors from a `Iterator<Result<T, E>>` via the
//!   [`stash_errors`][StashErrorsIter::stash_errors] method.
//! - Quick and easy aggregation of mixed error types via boxing and the [`BoxedStash`] type.
//! - Aggregation of strongly-typed error values via the [`TypedStash`] type.
//! - Fully custom wrapper error types can be produced via the [`TypedStash::with_constructor`]
//!   function.
//! - Simple customization of summary lines when using the builtin [`ErrorList`] type as your
//!   wrapper type.
//! - No dependencies other than `std`
//!   - `nostd` support is not currently provided, but will be considered if there is demand.
//!   - Tests validate compatibility with popular error handling crates, but the code doesn't depend
//!     on them.
//!
//! # Stash types
//!
//! This crate provides two types of error stashes:
//! - [`BoxedStash`]: A dynamically typed stash that can collect errors of any
//!   type that implements `std::error::Error + Send + Sync + 'static`, and
//!   wraps them in an [`ErrorList`].
//! - [`TypedStash`]: An error stash with generic type parameters that collects
//!   a specific type of child error and can emit them in a user-defined
//!   wrapper error type.
//!
//! Both types of stashes implement the [`ErrorStash`] trait, which provides
//! methods for adding errors to the stash, checking if any errors have been
//! collected, and emitting the collected errors as a single error result.
//!
//! This crate provides its own error aggregation type, [`ErrorList`], but
//! can also be used with your own error collection types via the
//! [`TypedStash::with_constructor`] function.
//!
//! # Result integration
//!
//! This crate provides two extension methods on `Result` via the
//! [`StashableResult`] trait:
//!
//! - [`or_stash(&mut stash)`][StashableResult::or_stash]: Converts a
//!   `Result<T, E>` into a `Option<T>`, adding any error `E` to the
//!   provided stash.
//! - [`or_fail(&mut stash)`][StashableResult::or_fail]: Converts a
//!   `Result<T, E>` into a `Result<T, W>`, where `W` is the stash's
//!   wrapper type and contains the current error and any errors
//!   previously collected by the stash.
//!
//! ## or_stash example
//!
//! ```
//! use std::num::{ParseIntError};
//! use errorstash::{TypedStash, StashableResult, ErrorList};
//!
//! fn sum_strings(a: &str, b: &str) -> Result<i32, ErrorList<ParseIntError>> {
//!     let mut stash = TypedStash::new();
//!     let a = a.parse::<i32>().or_stash(&mut stash);
//!     let b = b.parse::<i32>().or_stash(&mut stash);
//!     stash.fail_unless_empty()?;
//!     let sum = a.unwrap() + b.unwrap();
//!     Ok(sum)
//! }
//!
//! let r1 = sum_strings("10", "20");
//! let r2 = sum_strings("10", "foo");
//!
//! assert_eq!(r1.unwrap(), 30);
//! assert!(r2.is_err());
//! let errors: ErrorList<ParseIntError> = r2.unwrap_err();
//! assert_eq!(errors.len(), 1);
//! assert_eq!("invalid digit found in string", &errors[0].to_string());
//! ```
//!
//! As this example shows, when using [`or_stash`][StashableResult::or_stash], you
//! should only unwrap the returned `Option` values after calling verifying
//! that the stash is empty.
//!
//! # Iterator integration
//!
//! When processing many values using an iterator that produces `Result<T, E>` values,
//! you can stash all the errors using the [`stash_errors`][StashErrorsIter::stash_errors] method
//! of the [`StashErrorsIter`] extension trait:
//!
//! ```
//! use std::num::ParseIntError;
//! use errorstash::{TypedStash, StashErrorsIter};
//!
//! let inputs = vec!["42", "foo", "100", "bar"];
//! let mut stash = TypedStash::with_summary("Failed to parse some numbers:");
//! let numbers: Vec<i32> = inputs.into_iter()
//!     .map(|s| s.parse::<i32>())
//!     .stash_errors(&mut stash)
//!     .collect();
//!
//! assert_eq!(numbers, vec![42, 100]);
//! let errors: ErrorList<ParseIntError> = stash.to_error().unwrap();
//! assert_eq!(errors.len(), 2);
//! ```
mod boxed_stash;
mod error_list;
mod error_stash;
mod typed_stash;

pub use boxed_stash::{BoxedError, BoxedErrors, BoxedStash};
pub use error_list::ErrorList;
pub use error_stash::{ErrorStash, StashErrorsIter, StashableResult};
pub use typed_stash::TypedStash;
