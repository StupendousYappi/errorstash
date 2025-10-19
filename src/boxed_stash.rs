use std::error::Error;

use crate::error_list::{ErrorList, ErrorSummary};
use crate::error_stash::ErrorStashInternal;
use crate::{ErrorStash, StashableResult};

/// The child error type used by [`BoxedStash`] and [`BoxedErrors`].
///
/// This type is also used by the `anyhow` and `eyre` crates, allowing easy
/// conversions between BoxedError and the errors used by those crates.
pub type BoxedError = Box<dyn Error + Send + Sync + 'static>;

/// The wrapper error type produced by [`BoxedStash`].
///
/// A type alias for [`ErrorList`] with [`BoxedError`] as the inner error type.
pub type BoxedErrors = ErrorList<BoxedError>;

/// An [`ErrorStash`] that produces an [`ErrorList`] containing boxed, potentially
/// heterogeneous error values.
///
/// BoxedErrors is designed to be a simple output type for [`ErrorStash`] when
/// you want to collect multiple potential error types and report them together.
///
/// An [`ErrorStash`] that produces BoxedErrors values can be created using
/// the [`BoxedErrors::new_stash`] function.
///
/// The individual errors within a BoxedErrors will be instances of `Box<dyn
/// Error + Send + Sync + 'static>`, allowing most types that implements the
/// `Error` trait to be stored. These values themselves implement the `Error` trait,
/// and can be downcast back to their original types if needed.
///
/// # Terminal methods
///
/// Methods that can return a [`BoxedErrors`] (i.e. inside a `Result` or
/// `Option`) are considered terminal methods, as they consume the collected
/// errors to produce the wrapper error. In `BoxedStash`, these methods not only
/// consume all collected errors, they also reset the stash's summary line to
/// its default value.
///
/// Typically, this doesn't matter, because these methods
/// are normally called with the `?` operator, causing the calling function to
/// return and the stash to likely no longer be used if any errors ocurred. However,
/// if you do call a terminal method without propagating the error immediately
/// (i.e. without using `?`), be aware after the call the stash will be empty
/// and the summary line reset- if you intend to continue using the stash, you
/// may need to configure any custom summary line again.
///
/// The terminal methods on `BoxedStash` are:
/// - [`BoxedStash::to_result`]
/// - [`BoxedStash::to_error`]
/// - [`BoxedStash::fail_now`]
/// - [`BoxedStash::fail_unless_empty`]
///
/// # Compatibility
///
/// The two error types encountered when using BoxedStash are the wrapper error type,
/// [`BoxedErrors`], and the child error type, [`BoxedError`]. Both of these implement
/// `std::error::Error + Send + Sync + 'static`, making them compatible with other popular
/// error handling crates, including `anyhow`, `thiserror`, and `eyre`.
///
/// ## anyhow example
/// ```
/// use anyhow::{Error, anyhow};
/// # use errorstash::{ErrorStash, BoxedStash, BoxedErrors};
///
/// let mut stash = BoxedStash::new();
/// stash.set_summary("Found {count} errors:");
/// stash.push("a string error");
/// stash.push(anyhow!("an anyhow error"));
///
/// let wrapper = stash.to_error().unwrap();
/// let wrapper_as_anyhow: anyhow::Error = wrapper.into();
/// let wrapper_back: BoxedErrors = wrapper_as_anyhow.downcast().unwrap();
/// ```
///
/// ## thiserror example
/// ```
/// use thiserror::Error;
/// # use errorstash::{ErrorStash, BoxedStash, BoxedErrors};
///
/// #[derive(Debug, Error)]
/// enum CustomChildError {
///   #[error("Value is too big: {0}")]
///   TooBig(usize),
///   #[error("Value is too small: {0}")]
///   TooSmall(usize),
/// }
///
/// #[derive(Debug, Error)]
/// #[error(transparent)]
/// struct ValidationError(#[from] BoxedErrors);
///
/// let error = BoxedStash::new()
///     .set_summary("Found {count} errors:")
///     .push("a string error")
///     .fail_now(CustomChildError::TooBig(100))
///     .unwrap_err();
/// let result: Result<String, ValidationError> = Err(ValidationError(error));
/// let expected: &str = "
/// Found 2 errors:
/// - a string error
/// - Value is too big: 100
/// ";
/// assert_eq!(expected.trim(), result.unwrap_err().to_string().trim());
/// ```
///
/// The Rust standard library also implements `From<&str>` and `From<String>` for
/// `Box<dyn Error + Send + Sync + 'static>`, so `&str` and `String` values
/// can be passed directly to [`BoxedStash::push`] and [`BoxedStash::push_all`],
/// like this:
///
/// ```rust
/// use errorstash::{ErrorStash, BoxedStash, BoxedError, StashableResult, StashErrorsIter};
/// use anyhow::{Error, bail};
///
/// let mut stash = BoxedStash::new();
/// stash.set_summary("Found {count} errors:");
/// stash.push("the first error");
/// let error2: String = "another string error".to_string();
/// stash.push(error2);
///
/// fn fail_if_negative(x: i32) -> Result<i32, anyhow::Error> {
///    if x < 0 {
///      bail!("Negative value: {}", x);
///    }
///    Ok(x)
/// }
///
/// let values: Vec<i32> = vec![10, -3, 5, -1].into_iter()
///  .map(fail_if_negative)
///  .stash_errors(&mut stash)
///  .collect();
///
/// assert_eq!(vec![10, 5], values);
///
/// let wrapper = stash.to_error().unwrap();
/// assert_eq!(4, wrapper.len());
///
/// let expected: &str = "
/// Found 4 errors:
/// - the first error
/// - another string error
/// - Negative value: -3
/// - Negative value: -1
/// ";
///
/// assert_eq!(expected.trim(), wrapper.to_string().trim());
/// ```
///
/// # Formatting
///
/// The `std::fmt::Display` implementation for BoxedErrors prints them in this
/// format:
///
/// ```text
/// <summary line>
/// - <error 1>
/// - <error 2>
/// - <error 3>
/// ```
#[derive(Debug, Default)]
pub struct BoxedStash {
    summary: ErrorSummary,
    errors: Vec<BoxedError>,
}

impl ErrorStashInternal<BoxedError, BoxedErrors> for BoxedStash {
    fn errors(&self) -> &[BoxedError] {
        &self.errors
    }

    fn mut_errors(&mut self) -> &mut Vec<BoxedError> {
        &mut self.errors
    }

    fn consume(&mut self) -> BoxedErrors {
        let errors = std::mem::take(&mut self.errors);
        let summary = std::mem::take(&mut self.summary).with_count(errors.len());
        BoxedErrors::new(summary, errors)
    }
}

impl ErrorStash<BoxedError, BoxedErrors> for BoxedStash {
    fn to_result<T>(mut self, closure: impl FnOnce() -> T) -> Result<T, BoxedErrors> {
        ErrorStashInternal::to_result(&mut self, closure)
    }

    fn to_error(mut self) -> Option<BoxedErrors> {
        ErrorStashInternal::to_error(&mut self)
    }
}

impl BoxedStash {
    /// Creates a new BoxedStash with the default summary line.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new BoxedStash with the given summary line.
    pub fn with_summary(summary: &'static str) -> Self {
        let summary = ErrorSummary::new_static(summary);
        Self {
            summary,
            errors: Vec::new(),
        }
    }

    /// Sets a custom summary line for the wrapper error.
    ///
    /// If the summary string contains the `{count}` placeholder, it will be
    /// replaced with the number of errors when formatted.
    pub fn set_summary(&mut self, summary: &'static str) -> &mut Self {
        let summary = ErrorSummary::new_static(summary);
        self.summary = summary;
        self
    }

    /// Sets a custom summary line generator closure for the wrapper error.
    ///
    /// The closure will be called when the summary is formatted. If the
    /// returned string contains the `{count}` placeholder, it will be replaced
    /// with the number of errors when formatted.
    ///
    /// The provided closure may capture local state and must be `Send + Sync`
    /// so that the resulting [`BoxedStash`] remains thread-safe.
    pub fn set_summary_with(
        &mut self,
        summary_func: impl Fn() -> String + Send + Sync + 'static,
    ) -> &mut Self {
        let summary = ErrorSummary::new_dynamic(summary_func);
        self.summary = summary;
        self
    }

    /// Adds a child error to the stash.
    pub fn push(&mut self, err: impl Into<BoxedError>) -> &mut Self {
        self.mut_errors().push(err.into());
        self
    }

    /// Adds multiple child errors to the stash.
    pub fn push_all<T, It>(&mut self, errors: It) -> &mut Self
    where
        It: IntoIterator<Item = T>,
        T: Into<BoxedError>,
    {
        self.mut_errors().extend(errors.into_iter().map(Into::into));
        self
    }

    /// If the condition is false, adds error `e` to the stash. Otherwise,
    /// does nothing.
    ///
    /// If you want to return immediately if the condition is false,
    /// chain a call to [`fail_unless_empty`] after this method. For example:
    ///
    /// ```ignore
    /// let mut stash = BoxedStash::new();
    /// let value = 42;
    /// stash.check(value > 100, "value must be greater than 100")
    ///      .fail_unless_empty()?;
    /// ```
    pub fn check(&mut self, condition: bool, e: impl Into<BoxedError>) -> &mut Self {
        if !condition {
            self.mut_errors().push(e.into());
        }
        self
    }

    /// Adds an error and immediately returns `Err(W)` with all collected
    /// errors.
    pub fn fail_now(&mut self, e: impl Into<BoxedError>) -> Result<(), BoxedErrors> {
        self.mut_errors().push(e.into());
        let wrapper = self.consume();
        Err(wrapper)
    }
}

impl<T> Extend<T> for BoxedStash
where
    T: Into<BoxedError>,
{
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        // Map each incoming item into E and push into the internal vector.
        self.errors.extend(iter.into_iter().map(|t| t.into()));
    }
}

impl IntoIterator for BoxedStash {
    type Item = BoxedError;
    type IntoIter = std::vec::IntoIter<BoxedError>;

    fn into_iter(self) -> Self::IntoIter {
        self.errors.into_iter()
    }
}

/// Adds the ability to stash errors from a Result whose error type can be converted
/// into BoxedError.
///
/// Note that this implementation leverages the [`Into<BoxedError>`] trait to auto-convert
/// compatible error types into BoxedError. This behavior differs from the `StashableResult`
/// implementation for `TypedStash`, which requires the error type to match exactly.
impl<T, FE> StashableResult<T, BoxedError, ErrorList<BoxedError>, BoxedStash> for Result<T, FE>
where
    FE: Into<BoxedError>,
{
    fn or_stash(self, stash: &mut BoxedStash) -> Option<T> {
        match self {
            Ok(v) => Some(v),
            Err(e) => {
                let e = e.into();
                stash.mut_errors().push(e);
                None
            }
        }
    }

    fn or_fail(self, stash: &mut BoxedStash) -> Result<T, ErrorList<BoxedError>> {
        self.map_err(|e| {
            let e = e.into();
            stash.mut_errors().push(e);
            stash.consume()
        })
    }
}

/// Adds the ability to stash errors from a Result whose error type is a
/// Vec of the stash's child error type.
// impl<T, W, E> StashableResult<T, E, W, TypedStash<E, W>> for Result<T, Vec<E>>
// where
//     E: Display + Debug + Send + 'static,
//     W: Error + Send + 'static,
// {
//     fn or_stash(self, stash: &mut TypedStash<E, W>) -> Option<T> {
//         match self {
//             Ok(v) => Some(v),
//             Err(e) => {
//                 stash.mut_errors().extend(e.into_iter());
//                 None
//             }
//         }
//     }

//     fn or_fail(self, stash: &mut TypedStash<E, W>) -> Result<T, W> {
//         match self {
//             Ok(v) => Ok(v),
//             Err(e) => {
//                 stash.mut_errors().extend(e.into_iter());
//                 Err(stash.consume())
//             }
//         }
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;
    use test_log::test;
    use thiserror::Error;

    #[derive(Debug, Error)]
    enum CustomError {
        #[error("Error A: {0}")]
        A(String),

        #[error("Error B: {0}")]
        B(String),
    }

    #[test]
    fn basic_use() {
        let e1 = "first error";
        let e2 = "Second error".to_string();
        let e3 = CustomError::A("third error".to_string());

        let wrapper = BoxedStash::new()
            .set_summary("Found {count} errors")
            .push(e1)
            .push(e2)
            .fail_now(e3)
            .unwrap_err();

        assert_eq!(3, wrapper.len());
        assert_eq!("first error", wrapper[0].to_string());
        assert_eq!("Second error", wrapper[1].to_string());
        assert_eq!("Error A: third error", wrapper[2].to_string());

        let v = wrapper.to_vec();
        assert_eq!(3, v.len());
    }

    #[test]
    fn display_with_count_replacement() {
        let errors: Vec<Box<dyn Error + Send + Sync + 'static>> = vec![
            Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "IO error occurred",
            )),
            Box::new(CustomError::B("Custom error B occurred".to_string())),
        ];

        let error_list = BoxedErrors::new(
            "Multiple errors occurred: {count} issues found".into(),
            errors,
        );

        let display_output = format!("{}", error_list);

        let expected_output = "Multiple errors occurred: 2 issues found\n- IO error occurred\n- Error B: Custom error B occurred\n";
        assert_eq!(expected_output, display_output);
    }

    #[test]
    fn display_without_count_replacement() {
        let errors: Vec<Box<dyn Error + Send + Sync + 'static>> = vec![
            Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "IO error occurred",
            )),
            Box::new(CustomError::B("Custom error B occurred".to_string())),
        ];

        let error_list = BoxedErrors::new("Some errors occurred".into(), errors);

        let display_output = format!("{}", error_list);

        let expected_output =
            "Some errors occurred\n- IO error occurred\n- Error B: Custom error B occurred\n";
        assert_eq!(expected_output, display_output);
    }

    #[test]
    fn push_all_with_strs() {
        let strs = vec!["error one", "error two", "error three"];
        let mut stash = BoxedStash::new();
        stash.push_all(strs.iter().copied());
        let errors = stash.consume();
        assert_eq!(errors.len(), 3);
        assert_eq!(errors[0].to_string(), "error one");
        assert_eq!(errors[1].to_string(), "error two");
        assert_eq!(errors[2].to_string(), "error three");
    }

    #[test]
    fn summary_returns_raw_summary_with_placeholder() {
        let errors: Vec<_> = vec!["first error".into(), "second error".into()];
        let error_list = BoxedErrors::new("Found {count} problems".into(), errors);
        assert_eq!("Found 2 problems", error_list.summary());
    }

    #[test]
    fn summary_returns_raw_summary_without_placeholder() {
        let errors: Vec<BoxedError> = vec![Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "IO error",
        ))];
        let error_list = BoxedErrors::new("Some summary text".into(), errors);
        assert_eq!("Some summary text", error_list.summary());
    }

    #[test]
    fn extend_with_str_iterator() {
        let strs = vec!["error a", "error b", "error c"];
        let mut stash = BoxedStash::new();
        stash.push("initial error");
        stash.extend(strs.iter().copied());
        let errors = stash.consume();
        assert_eq!(errors.len(), 4);
        assert_eq!(errors[0].to_string(), "initial error");
        assert_eq!(errors[1].to_string(), "error a");
        assert_eq!(errors[2].to_string(), "error b");
        assert_eq!(errors[3].to_string(), "error c");
    }

    #[test]
    fn push_all_with_str_iterator() {
        let strs = vec!["error x", "error y", "error z"];
        let mut stash = BoxedStash::new();
        stash.push("first error");
        stash.push_all(strs.iter().copied());
        let errors = stash.consume();
        assert_eq!(errors.len(), 4);
        assert_eq!(errors[0].to_string(), "first error");
        assert_eq!(errors[1].to_string(), "error x");
        assert_eq!(errors[2].to_string(), "error y");
        assert_eq!(errors[3].to_string(), "error z");
    }

    #[test]
    fn summary_with_capturing_closure() {
        let prefix = "errors detected";
        let mut stash = BoxedStash::new();
        stash.set_summary_with(move || prefix.to_ascii_uppercase() + ": {count}");
        stash.push("boom");

        let errors = stash.consume();
        assert_eq!(1, errors.len());
        assert_eq!("ERRORS DETECTED: 1", errors.summary());

        let summary_after = stash.push("test").to_error().unwrap().summary();

        assert_eq!("Encountered 1 errors:", summary_after);
    }

    #[test]
    fn check_fmt_adds_formatted_error_when_false() {
        let mut stash = BoxedStash::new();
        let value = 42;
        // condition is false, so the formatted message should be pushed
        stash.check_fmt(
            value > 100,
            format_args!("value {} is not greater than 100", value),
        );
        let errors = stash.consume();
        assert_eq!(1, errors.len());
        assert_eq!("value 42 is not greater than 100", errors[0].to_string());
    }

    #[test]
    fn check_fmt_noop_when_true() {
        let mut stash = BoxedStash::new();
        let value = 200;
        // condition is true, so no error should be added
        stash.check_fmt(
            value > 100,
            format_args!("value {} is not greater than 100", value),
        );
        assert_eq!(0, stash.len());
    }
}
