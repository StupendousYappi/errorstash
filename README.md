# errorstash

A library for collecting multiple related errors, and reporting them together.

```rust
use std::collections::HashSet;
use url::Url;
use thiserror::Error;
use errorstash::{BoxedStash, BoxedErrors, OrStash};

#[derive(Error, Debug)]
#[error("Domain '{0}' is not trusted")]
struct InvalidDomainError(String);

#[derive(Debug, PartialEq, Eq)]
struct TrustedUrl(String);

#[derive(Debug)]
struct UrlValidator(HashSet<String>);

impl UrlValidator {
    fn new(trusted_domains: &[String]) -> Self {
        Self(trusted_domains.into_iter().collect())
    }

    /// Validates that the given URL complies with our requirements, and if
    /// so, returns its domain as a `TrustedUrl`.
    fn validate_url(&self, url: &str) -> Result<TrustedUrl, BoxedErrors> {
        let mut stash = BoxedStash::with_summary("Invalid URL:");
        let parsed = Url::parse(url)
            .or_fail(&mut stash)?;

        stash
            .check(parsed.scheme() == "https", "URL must use https schema")
            .check(parsed.port().is_none(), "URL must not specify a port");
        let raw_domain = parsed.host_str().unwrap_or_default();
        if raw_domain.is_empty() {
            stash.push("URL must have a domain");
        } else if !self.0.contains(raw_domain) {
            let e = InvalidDomainError(raw_domain.into());
            stash.push(e);
        }
        stash.fail_unless_empty()?;

        let as_safe = TrustedUrl(url.into());
        Ok(as_safe)
    }
}
```

[![Crates.io](https://img.shields.io/crates/v/errorstash.svg)](https://crates.io/crates/errorstash)
[![Docs.rs](https://docs.rs/errorstash/badge.svg)](https://docs.rs/errorstash)
[![CI](https://github.com/StupendousYappi/errorstash/workflows/CI/badge.svg)](https://github.com/StupendousYappi/errorstash/actions)

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

See [CONTRIBUTING.md](CONTRIBUTING.md).