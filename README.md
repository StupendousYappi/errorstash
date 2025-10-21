# errorstash

A library for collecting multiple related errors, and reporting them together.

Error stashes are useful when you want to perform multiple operations that may
independently fail, and you want to collect all errors that occur and return
them together, rather than failing fast on the first error. For example, when
validating data, you may want to provide the caller with a complete list of
validation errors so that they can fix them all at once.

```rust
use std::collections::HashSet;
use url::Url;
use thiserror::Error;
use errorstash::{ErrorStash, BoxedStash, BoxedErrorList, StashableResult};

#[derive(Error, Debug)]
#[error("Domain '{0}' is not trusted")]
struct InvalidDomainError(String);

#[derive(Debug, PartialEq, Eq)]
struct TrustedUrl(Url);

#[derive(Debug, Clone, PartialEq, Eq)]
struct UrlValidator(HashSet<String>);

impl UrlValidator {
    fn new(trusted_domains: Vec<String>) -> Self {
        Self(trusted_domains.into_iter().collect())
    }

    /// Validates that the given URL complies with our requirements, and if
    /// so, returns it as a `TrustedUrl`.
    fn validate_url(&self, url: &str) -> Result<TrustedUrl, BoxedErrorList> {
        let mut stash = BoxedStash::with_summary("Invalid URL:");
        let parsed = Url::parse(url).or_fail(&mut stash)?;

        stash
            .check(parsed.scheme() == "https", "URL must use https scheme")
            .check(parsed.port().is_none(), "URL must not specify a port");
        let raw_domain = parsed.host_str().unwrap_or_default();
        if raw_domain.is_empty() {
            stash.push("URL must have a domain");
        } else if !self.0.contains(raw_domain) {
            let e = InvalidDomainError(raw_domain.into());
            stash.push(e);
        }
        stash.fail_unless_empty()?;

        let as_safe = TrustedUrl(parsed);
        Ok(as_safe)
    }
}
```

This crate defines an
[`ErrorStash`](https://docs.rs/errorstash/latest/errorstash/trait.ErrorStash.html)
trait along with two struct implementations:
- [`BoxedStash`](https://docs.rs/errorstash/latest/errorstash/struct.BoxedStash.html)
  a stash that wraps its child errors in a `Box<dyn Error + Send + Sync +
'static>`, allowing a mix of different child error types, and emitting them together in
an
[`ErrorList`](https://docs.rs/errorstash/latest/errorstash/struct.ErrorList.html)
- [`TypedStash`](https://docs.rs/errorstash/latest/errorstash/struct.TypedStash.html)
  a stash with a generic type for its child errors, requiring all errors to have
the same type, and allowing for custom wrapper error types

[![Crates.io](https://img.shields.io/crates/v/errorstash.svg)](https://crates.io/crates/errorstash)
[![Docs.rs](https://docs.rs/errorstash/badge.svg)](https://docs.rs/errorstash)
[![CI](https://github.com/StupendousYappi/errorstash/workflows/CI/badge.svg)](https://github.com/StupendousYappi/errorstash/actions)

## Related projects

This project was inspired by
[lazy_errors](https://github.com/Lintermute/lazy_errors), including the error
stash concept and the integration with the `Result` and `Iterator` types. This
project attempts to offer a simpler API and support for fully custom wrapper
error types.

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.