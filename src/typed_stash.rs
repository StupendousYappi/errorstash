use std::error::Error;
use std::fmt::{Debug, Display, Formatter};

use crate::error_list::{DEFAULT_SUMMARY, ErrorList};
use crate::error_stash::ErrorStashInternal;
use crate::{ErrorStash, StashableResult};

/// Internal enum for the constructor implementation of the wrapper error.
///
/// Custom user-provided logic is stored as a boxed closure, while the common
/// case stashes that emit an `ErrorList` can use a static summary string and
/// avoid any heap allocations.
enum WrapperConstructor<E, W>
where
    E: Debug + Display + Send + 'static,
    W: Error + Send + 'static,
{
    StaticStr(&'static str, fn(Vec<E>, &'static str) -> W),
    Closure(Box<dyn Fn(Vec<E>) -> W + Send + 'static>),
}

impl<E, W> WrapperConstructor<E, W>
where
    E: Debug + Display + Send + 'static,
    W: Error + Send + 'static,
{
    fn construct(&self, errors: Vec<E>) -> W {
        match self {
            WrapperConstructor::StaticStr(summary, f) => f(errors, summary),
            WrapperConstructor::Closure(f) => f(errors),
        }
    }

    fn for_closure(f: impl Fn(Vec<E>) -> W + Send + 'static) -> Self {
        WrapperConstructor::Closure(Box::new(f))
    }

    fn for_static_str(summary: &'static str, f: fn(Vec<E>, &'static str) -> W) -> Self {
        WrapperConstructor::StaticStr(summary, f)
    }
}

/// Collects child errors of a specific type, then produces a a wrapper
/// error if any errors were collected.
///
/// TypedStash, unlike [`BoxedStash`], requires all its child errors to have the
/// same type. This allows its output wrapper errors to be strongly typed,
/// avoids heap allocations for boxing, and allows you to to use your own custom
/// wrapper error types by providing a constructor function.
///
/// Note that the methods for adding errors to the stash (e.g.
/// [`TypedStash::push`], [`TypedStash::fail_now`],
/// [`StashableResult::or_stash`]) take the child error type `E` directly, whereas
/// the [`BoxedStash`] methods take `Into<BoxedError>`.
///
/// # Terminal methods
///
/// Methods that can return the wrapper error type `W` (i.e. inside a `Result` or
/// `Option`) are considered terminal methods, as they consume the collected
/// errors to produce the wrapper error.
///
/// Typically, this doesn't matter, because these methods are normally called
/// with the `?` operator, causing the calling function to return and the stash
/// to likely no longer be used if any errors ocurred. However, if you do call a
/// terminal method without propagating the error immediately (i.e. without
/// using `?`), be aware after the call the stash will be empty.
///
/// The terminal methods on [`TypedStash`] are:
/// - [`TypedStash::to_result`]
/// - [`TypedStash::to_error`]
/// - [`TypedStash::fail_now`]
/// - [`TypedStash::fail_unless_empty`]
///
/// # Generic types
///
/// `W` is the wrapper error type that must implement [`std::error::Error`].
/// `E` is the child error type that must implement [`std::fmt::Display`] and [`std::fmt::Debug`].
pub struct TypedStash<E, W>
where
    E: Debug + Display + Send + 'static,
    W: Error + Send + 'static,
{
    errors: Vec<E>,
    constructor: WrapperConstructor<E, W>,
}

impl<E, W> Debug for TypedStash<E, W>
where
    E: Debug + Display + Send + 'static,
    W: Error + Send + 'static,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypedStash")
            .field("errors", &self.errors)
            .finish()
    }
}

impl<E, W> Display for TypedStash<E, W>
where
    E: Debug + Display + Send + 'static,
    W: Error + Send + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TypedStash with {} errors", self.errors.len())
    }
}

impl<E, W> ErrorStashInternal<E, W> for TypedStash<E, W>
where
    E: Debug + Display + Send + 'static,
    W: Error + Send + 'static,
{
    fn errors(&self) -> &[E] {
        &self.errors
    }

    fn mut_errors(&mut self) -> &mut Vec<E> {
        &mut self.errors
    }

    fn consume(&mut self) -> W {
        let errors = std::mem::take(&mut self.errors);
        self.constructor.construct(errors)
    }
}

impl<E, W> ErrorStash<E, W> for TypedStash<E, W>
where
    E: Debug + Display + Send + 'static,
    W: Error + Send + 'static,
{
    fn to_result<T>(mut self, closure: impl FnOnce() -> T) -> Result<T, W> {
        ErrorStashInternal::to_result(&mut self, closure)
    }

    /// If no errors have been collected, returns `None`. Otherwise,
    /// consumes the collected errors and returns `Some(W)`.
    fn to_error(mut self) -> Option<W> {
        ErrorStashInternal::to_error(&mut self)
    }
}

impl<E> TypedStash<E, ErrorList<E>>
where
    E: Debug + Display + Send + 'static,
{
    /// Creates a new stash that produces an [`ErrorList`] with a default
    /// summary message if any errors are collected.
    ///
    /// The stash will use the default summary message "Encountered N errors:"
    /// in any produced [`ErrorList`] values, where N is the number of collected
    /// errors.
    /// ```
    /// # use errorstash::{ErrorStash, TypedStash};
    /// // Create an empty `TypedStash<String, ErrorList<String>>`, with
    /// // the child error type determined via type inference.
    /// let mut stash = TypedStash::new();
    /// assert!(stash.is_empty());
    /// stash.push("some error".to_string());
    /// stash.push("another error".to_string());
    /// assert!(!stash.is_empty());
    /// let error = stash.to_error().unwrap();
    /// assert_eq!(error.summary(), "Encountered 2 errors:");
    /// ```
    pub fn new() -> Self {
        Self::with_summary(DEFAULT_SUMMARY)
    }

    /// Creates a new stash that produces an [`ErrorList`] with the provided
    /// static summary message if any errors are collected.
    ///
    /// If the summary string contains the placeholder `{count}`, it will be
    /// replaced with the number of collected errors when the summary is
    /// formatted.
    ///
    /// ```
    /// # use errorstash::{ErrorStash, TypedStash};
    /// let mut stash = TypedStash::with_summary("Input contains {count} invalid record(s):");
    /// assert!(stash.is_empty());
    /// stash.push("some error".to_string());
    /// assert_eq!(stash.len(), 1);
    /// let error = stash.to_error().unwrap();
    /// assert_eq!(error.summary(), "Input contains 1 invalid record(s):");
    /// ```
    pub fn with_summary(summary: &'static str) -> Self {
        let constructor = WrapperConstructor::for_static_str(summary, Self::create_error_list);
        Self {
            errors: Vec::new(),
            constructor,
        }
    }

    fn create_error_list(errors: Vec<E>, summary: &'static str) -> ErrorList<E> {
        ErrorList::new(summary.into(), errors)
    }
}

// We intentionally don't use `Into<E>` for argument types here (unlike in
// BoxedStash) to enable better type inference for TypedStash types. BoxedStash
// has no generic type parameters, so doesn't need type inference help.
impl<E, W> TypedStash<E, W>
where
    E: Debug + Display + Send + 'static,
    W: Error + Send + 'static,
{
    /// Creates a new stash that will use the given constructor
    /// to produce the wrapper error if any errors are collected.
    ///
    /// The constructor function takes a `Vec<E>` of the collected
    /// child errors and produces a `W` wrapper error.
    ///
    /// ## Example
    /// ```
    /// # use errorstash::{ErrorStash, TypedStash};
    /// # fn get_product_id(_: &str) -> u32 { 0 }
    /// use std::fmt::Debug;
    /// use thiserror::Error;
    ///
    /// let product_name = "widget".to_string();
    /// let product_id: u32 = get_product_id(&product_name);
    ///
    /// #[derive(Error, Debug)]
    /// #[error("Product errors for {product_name} (ID {product_id}): {errors:?}")]
    /// struct ProductError {
    ///   product_id: u32,
    ///   product_name: String,
    ///   errors: Vec<&'static str>,
    /// }
    ///
    /// let mut stash = TypedStash::with_constructor(move |errors| {
    ///   ProductError {
    ///     errors,
    ///     product_id,
    ///     product_name: product_name.clone(),
    ///   }
    /// });
    ///
    /// stash.push("Faulty sprockets");
    /// let result = stash.to_error();
    /// let err: ProductError = result.unwrap();
    ///
    /// assert_eq!(err.product_id, product_id);
    /// assert_eq!(err.product_name, "widget");
    /// assert_eq!(err.errors, vec!["Faulty sprockets"]);
    /// ```
    ///
    pub fn with_constructor<F>(constructor: F) -> Self
    where
        F: Fn(Vec<E>) -> W + Send + 'static,
    {
        Self {
            errors: Vec::new(),
            constructor: WrapperConstructor::for_closure(constructor),
        }
    }

    /// Adds a child error to the stash.
    ///
    /// ```
    /// # use errorstash::{ErrorStash, TypedStash};
    /// let mut stash = TypedStash::new();
    ///
    /// assert_eq!(stash.len(), 0);
    /// stash.push("some error".to_string());
    /// assert_eq!(stash.len(), 1);
    /// ```
    pub fn push(&mut self, err: E) -> &mut Self {
        self.mut_errors().push(err);
        self
    }

    /// Adds multiple child errors from an iterator to the stash.
    ///
    /// ```
    /// # use errorstash::{ErrorStash, TypedStash};
    /// let mut stash = TypedStash::new();
    /// assert_eq!(stash.len(), 0);
    /// let errors = vec!["error one".to_string(), "error two".to_string()];
    /// stash.push_all(errors);
    /// assert_eq!(stash.len(), 2);
    /// ```
    pub fn push_all<It>(&mut self, errors: It) -> &mut Self
    where
        It: IntoIterator<Item = E>,
    {
        self.mut_errors().extend(errors);
        self
    }

    /// If the condition is false, adds error `e` to the stash. Otherwise,
    /// does nothing.
    ///
    /// If you want to return immediately if the condition is false,
    /// chain a call to [`ErrorStash::fail_unless_empty`] after this method. For example:
    ///
    /// ```ignore
    /// let mut stash = BoxedStash::new();
    /// let value = 42;
    /// stash.check(value > 100, "value must be greater than 100")
    ///      .fail_unless_empty()?;
    /// ```
    pub fn check(&mut self, condition: bool, e: E) -> &mut Self {
        if !condition {
            self.mut_errors().push(e);
        }
        self
    }

    /// Adds an error and immediately returns `Err(W)` with all collected
    /// errors.
    pub fn fail_now(&mut self, e: E) -> Result<(), W> {
        self.mut_errors().push(e);
        let wrapper = self.consume();
        Err(wrapper)
    }
}

impl<E, W> From<TypedStash<E, W>> for Option<W>
where
    E: Debug + Display + Send + 'static,
    W: Error + Send + 'static,
{
    fn from(stash: TypedStash<E, W>) -> Self {
        stash.to_error()
    }
}

impl<E, W> From<TypedStash<E, W>> for Vec<E>
where
    E: Display + Debug + Send + 'static,
    W: Error + Send + 'static,
{
    fn from(stash: TypedStash<E, W>) -> Self {
        stash.errors
    }
}

/// Creates a new `TypedStash` that emits [`ErrorList`] wrapper errors
/// with the default summary message if any errors are collected.
impl<E> Default for TypedStash<E, ErrorList<E>>
where
    E: Debug + Display + Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

// Allow TypedStash to be extended from any iterator producing items that
// can be converted into the stash's error type `E`.
impl<E, W, T> Extend<T> for TypedStash<E, W>
where
    E: Display + Debug + Send + 'static,
    W: Error + Send + 'static,
    T: Into<E>,
{
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        // Map each incoming item into E and push into the internal vector.
        self.errors.extend(iter.into_iter().map(|t| t.into()));
    }
}

/// Allows consuming the stash to iterate over its collected errors.
///
/// This trait allows one stash to be added to another via the [`TypedStash::push_all`]
/// method.
impl<E, W> IntoIterator for TypedStash<E, W>
where
    E: Display + Debug + Send + 'static,
    W: Error + Send + 'static,
{
    type Item = E;
    type IntoIter = std::vec::IntoIter<E>;

    fn into_iter(self) -> Self::IntoIter {
        self.errors.into_iter()
    }
}

/// Adds the ability to stash errors from a Result whose error type matches
/// the stash's.
///
/// Note that this implementation requires the error types of the stash and
/// the result to match. This differs from the `StashableResult` implementation
/// for `BoxedStash`, which leverages the `Into<BoxedError>` trait to
/// auto-convert compatible error types.
impl<T, W, E> StashableResult<T, E, W, TypedStash<E, W>> for Result<T, E>
where
    E: Display + Debug + Send + 'static,
    W: Error + Send + 'static,
{
    fn or_stash(self, stash: &mut TypedStash<E, W>) -> Option<T> {
        match self {
            Ok(v) => Some(v),
            Err(e) => {
                stash.mut_errors().push(e);
                None
            }
        }
    }

    fn or_fail(self, stash: &mut TypedStash<E, W>) -> Result<T, W> {
        self.map_err(|e| {
            stash.mut_errors().push(e);
            stash.consume()
        })
    }
}

/// Adds the ability to stash errors from a Result whose error type is an
/// ErrorList of the stash's child error type.
///
/// The methods of this trait implementation add all the child child errors
/// from this result's [`ErrorList`] to the stash. The ErrorList's summary
/// message is lost in the process- only the summary from the destination
/// stash, if any, will be used.
///
/// The capability is currently only implemented for TypedStash, not BoxedStash,
impl<T, W, E> StashableResult<T, E, W, TypedStash<E, W>> for Result<T, ErrorList<E>>
where
    E: Display + Debug + Send + 'static,
    W: Error + Send + 'static,
{
    fn or_stash(self, stash: &mut TypedStash<E, W>) -> Option<T> {
        match self {
            Ok(v) => Some(v),
            Err(e) => {
                stash.mut_errors().extend(e);
                None
            }
        }
    }

    fn or_fail(self, stash: &mut TypedStash<E, W>) -> Result<T, W> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => {
                stash.mut_errors().extend(e);
                Err(stash.consume())
            }
        }
    }
}

/// Adds the ability to stash errors from a Result whose error type is a
/// Vec of the stash's child error type.
///
/// The methods of this trait implementation add all the child child errors
/// from this result's Vec to the stash. The ErrorList's summary
/// message is lost in the process- only the summary from the destination
/// stash, if any, will be used.
///
/// The capability is currently only implemented for TypedStash, not BoxedStash,
impl<T, W, E> StashableResult<T, E, W, TypedStash<E, W>> for Result<T, Vec<E>>
where
    E: Display + Debug + Send + 'static,
    W: Error + Send + 'static,
{
    fn or_stash(self, stash: &mut TypedStash<E, W>) -> Option<T> {
        match self {
            Ok(v) => Some(v),
            Err(e) => {
                stash.mut_errors().extend(e);
                None
            }
        }
    }

    fn or_fail(self, stash: &mut TypedStash<E, W>) -> Result<T, W> {
        match self {
            Ok(v) => Ok(v),
            Err(e) => {
                stash.mut_errors().extend(e);
                Err(stash.consume())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Display;

    use super::*;
    use crate::{StashErrorsIter, StashableResult};

    #[derive(Debug)]
    struct CustomError {
        msg: String,
    }

    impl Display for CustomError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.msg)
        }
    }

    impl CustomError {
        fn new(msg: String) -> Self {
            Self { msg }
        }
    }

    impl std::error::Error for CustomError {}

    macro_rules! oops {
        ($fmt:literal $(, $args:expr)* $(,)?) => {
            CustomError::new(format!($fmt $(, $args)*))
        };
    }

    // For the tests, treat AppError as our local CustomError type so the
    // wrapper types store Vec<CustomError> instead of anyhow::Error.
    type AppError = CustomError;

    // ...existing code...

    // Reusable wrapper error type for tests that use `String` as the child
    // error type for `ErrorStash`.
    #[derive(Debug)]
    struct StringErrorWrapper {
        errors: Vec<String>,
    }

    impl std::fmt::Display for StringErrorWrapper {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            for err in &self.errors {
                writeln!(f, "- {}", err)?;
            }
            Ok(())
        }
    }

    impl std::error::Error for StringErrorWrapper {}

    impl StringErrorWrapper {
        fn new(errors: Vec<String>) -> Self {
            Self { errors }
        }
    }

    // Small shared newtype that can be converted into `String` for tests.
    struct SimpleInt(usize);

    impl From<SimpleInt> for String {
        fn from(v: SimpleInt) -> Self {
            v.0.to_string()
        }
    }

    #[derive(Debug)]
    struct WrapperError {
        errors: Vec<AppError>,
    }

    impl Display for WrapperError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            for error in &self.errors {
                writeln!(f, "- {}", error)?;
            }
            Ok(())
        }
    }

    impl WrapperError {
        fn new(errors: Vec<AppError>) -> Self {
            Self { errors }
        }
    }

    impl std::error::Error for WrapperError {}

    #[test]
    fn returns_ok_when_no_errors() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        let result = collector.fail_unless_empty();
        assert!(result.is_ok());
    }

    #[test]
    fn returns_err_with_collected_errors() {
        let mut collector = TypedStash::new();

        collector.push(oops!("first"));
        collector.push(oops!("second"));

        let result = collector.fail_unless_empty();

        let messages = result
            .unwrap_err()
            .into_iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>();

        assert_eq!(messages, vec!["first", "second"]);
    }

    #[test]
    fn is_empty_returns_true_for_new_stash() {
        let collector = TypedStash::<String, _>::new();
        assert!(collector.is_empty());
    }

    #[test]
    fn is_empty_returns_false_after_adding_error() {
        let mut collector = TypedStash::new();
        collector.push(oops!("error"));
        assert!(!collector.is_empty());
    }

    #[test]
    fn errors_returns_empty_slice_initially() {
        let collector = TypedStash::<String, _>::new();
        assert_eq!(collector.len(), 0);
    }

    #[test]
    fn errors_returns_slice_of_collected_errors() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        collector.push(oops!("first"));
        collector.push(oops!("second"));

        let errors = collector.errors();
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].to_string(), "first");
        assert_eq!(errors[1].to_string(), "second");
    }

    #[test]
    fn push_adds_error_and_returns_self() {
        let mut collector = TypedStash::new();
        let result = collector.push(oops!("test error"));

        // push returns &mut Self for chaining
        assert!(!result.is_empty());
        let errors = result.to_error().unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].to_string(), "test error");
    }

    #[test]
    fn push_can_be_chained() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        collector
            .push(oops!("first"))
            .push(oops!("second"))
            .push(oops!("third"));

        let errors = collector.to_error().unwrap().errors;
        assert_eq!(errors.len(), 3);
        assert_eq!(errors[0].to_string(), "first");
        assert_eq!(errors[1].to_string(), "second");
        assert_eq!(errors[2].to_string(), "third");
    }

    #[test]
    fn to_result_returns_ok_when_no_errors() {
        let collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        let result = collector.to_result(|| "success");

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }

    #[test]
    fn to_result_returns_err_when_errors_exist() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        collector.push(oops!("error1"));
        collector.push(oops!("error2"));

        let result = collector.to_result(|| "success");

        assert!(result.is_err());
        let wrapper_error = result.unwrap_err();
        assert_eq!(wrapper_error.errors.len(), 2);
        assert_eq!(wrapper_error.errors[0].to_string(), "error1");
        assert_eq!(wrapper_error.errors[1].to_string(), "error2");
    }

    #[test]
    fn to_error_returns_none_when_no_errors() {
        let collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        let result = collector.to_error();

        assert!(result.is_none());
    }

    #[test]
    fn to_error_returns_some_when_errors_exist() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        collector.push(oops!("error1"));
        collector.push(oops!("error2"));

        let result = collector.to_error();

        assert!(result.is_some());
        let wrapper_error = result.unwrap();
        assert_eq!(wrapper_error.errors.len(), 2);
        assert_eq!(wrapper_error.errors[0].to_string(), "error1");
        assert_eq!(wrapper_error.errors[1].to_string(), "error2");
    }

    #[test]
    fn ok_returns_some_when_no_errors() {
        let collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        // ok should return Some(()) when no errors have been collected
        assert_eq!(collector.ok(), Some(()));
    }

    #[test]
    fn ok_returns_none_when_errors_exist() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        collector.push(oops!("an error"));
        // ok should return None when there are collected errors
        assert_eq!(collector.ok(), None);
    }

    #[test]
    fn fail_now_returns_err_and_consumes_errors_when_empty() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);

        let res = collector.fail_now(oops!("single"));

        assert!(res.is_err());
        let wrapper = res.unwrap_err();
        // should contain the single pushed error
        assert_eq!(wrapper.errors.len(), 1);
        assert_eq!(wrapper.errors[0].to_string(), "single");

        // stash should have been consumed
        assert!(collector.errors().is_empty());
    }

    #[test]
    fn fail_now_returns_err_and_consumes_prior_errors() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        collector.push(oops!("first"));
        collector.push(oops!("second"));

        let res = collector.fail_now(oops!("third"));

        assert!(res.is_err());
        let wrapper = res.unwrap_err();
        // should contain the three errors in order
        let msgs: Vec<String> = wrapper.errors.into_iter().map(|e| e.to_string()).collect();
        assert_eq!(msgs, vec!["first", "second", "third"]);

        // stash should have been consumed
        assert!(collector.is_empty());
    }

    // Tests for OrStash trait

    #[test]
    fn or_stash_returns_some_for_ok_result() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        let result: Result<String, AppError> = Ok("success".to_string());

        let value = result.or_stash(&mut collector);

        assert_eq!(value, Some("success".to_string()));
        assert!(collector.is_empty());
    }

    #[test]
    fn or_stash_collects_error_and_returns_none() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        let result: Result<String, AppError> = Err(oops!("test error"));

        let value = result.or_stash(&mut collector);

        assert!(value.is_none());
        assert!(!collector.is_empty());
        assert_eq!(collector.len(), 1);
    }

    #[test]
    fn or_stash_can_collect_multiple_errors() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);

        let result1: Result<i32, AppError> = Err(oops!("first error"));
        let result2: Result<String, AppError> = Ok("success".to_string());
        let result3: Result<i32, AppError> = Err(oops!("second error"));

        let value1 = result1.or_stash(&mut collector);
        let value2 = result2.or_stash(&mut collector);
        let value3 = result3.or_stash(&mut collector);

        assert!(value1.is_none());
        assert_eq!(value2, Some("success".to_string()));
        assert!(value3.is_none());

        assert_eq!(collector.errors().len(), 2);
        assert_eq!(collector.errors()[0].to_string(), "first error");
        assert_eq!(collector.errors()[1].to_string(), "second error");
    }

    #[test]
    fn or_stash_with_iterator_filter_map() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);

        let results = vec![
            Ok(1),
            Err(oops!("error 1")),
            Ok(2),
            Err(oops!("error 2")),
            Ok(3),
        ];

        let values: Vec<i32> = results
            .into_iter()
            .filter_map(|r| r.or_stash(&mut collector))
            .collect();

        assert_eq!(values, vec![1, 2, 3]);
        assert_eq!(collector.errors().len(), 2);
        assert_eq!(collector.errors()[0].to_string(), "error 1");
        assert_eq!(collector.errors()[1].to_string(), "error 2");
    }

    #[test]
    fn or_fail_ok_when_stash_has_prior_errors() {
        // Create a TypedStash that produces WrapperError from AppError (CustomError)
        let mut stash: TypedStash<AppError, WrapperError> =
            TypedStash::with_constructor(WrapperError::new);

        // Add a prior error
        stash.push(oops!("previous"));

        // An Ok result should return Ok even though stash already has errors
        let ok_result: Result<i32, AppError> = Ok(42);
        let ok = ok_result.or_fail(&mut stash);
        assert!(ok.is_ok());
        assert_eq!(ok.unwrap(), 42);

        // The stash should remain unchanged after or_fail on Ok
        assert_eq!(stash.len(), 1);
    }

    #[test]
    fn or_fail_err_includes_prior_and_new_error() {
        let mut stash: TypedStash<AppError, WrapperError> =
            TypedStash::with_constructor(WrapperError::new);
        stash.push(oops!("previous"));

        // An Err result should return Err and include both the prior and the new error
        let err_result: Result<i32, AppError> = Err(oops!("second"));
        let res = err_result.or_fail(&mut stash);
        assert!(res.is_err());

        let wrapper = res.unwrap_err();
        let messages: Vec<String> = wrapper.errors.into_iter().map(|e| e.to_string()).collect();

        assert_eq!(messages, vec!["previous", "second"]);
    }

    // Tests for StashErrors trait

    #[test]
    fn stash_errors_with_empty_iterator() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        let results: Vec<Result<i32, AppError>> = vec![];

        let values: Vec<i32> = results.into_iter().stash_errors(&mut collector).collect();

        assert_eq!(values, Vec::<i32>::new());
        assert!(collector.is_empty());
    }

    #[test]
    fn stash_errors_with_all_ok_values() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        let results: Vec<Result<_, AppError>> = vec![Ok(1), Ok(2), Ok(3)];

        let values: Vec<i32> = results.into_iter().stash_errors(&mut collector).collect();

        assert_eq!(values, vec![1, 2, 3]);
        assert!(collector.is_empty());
    }

    #[test]
    fn stash_errors_enables_type_inference() {
        // the calls to `or_stash` enable rust to infer the error type of the stash
        let mut collector = TypedStash::new();
        let results = vec![Ok(1), Ok(2), Err(oops!("error")), Ok(3)];
        let mut sum: i32 = 0;
        for result in results {
            let value = result.or_stash(&mut collector);
            if let Some(v) = value {
                sum += v;
            }
        }
        assert_eq!(sum, 6);
        let result = collector.fail_unless_empty();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].to_string(), "error");
    }

    #[test]
    fn stash_errors_with_all_err_values() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        let results = vec![
            Err(oops!("error 1")),
            Err(oops!("error 2")),
            Err(oops!("error 3")),
        ];

        let values: Vec<i32> = results.into_iter().stash_errors(&mut collector).collect();

        assert_eq!(values, Vec::<i32>::new());
        assert_eq!(collector.errors().len(), 3);
        assert_eq!(collector.errors()[0].to_string(), "error 1");
        assert_eq!(collector.errors()[1].to_string(), "error 2");
        assert_eq!(collector.errors()[2].to_string(), "error 3");
    }

    #[test]
    fn stash_errors_with_mixed_results() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        let results = vec![
            Ok(1),
            Err(oops!("first error")),
            Ok(2),
            Ok(3),
            Err(oops!("second error")),
            Ok(4),
        ];

        let values: Vec<i32> = results.into_iter().stash_errors(&mut collector).collect();

        assert_eq!(values, vec![1, 2, 3, 4]);
        assert_eq!(collector.errors().len(), 2);
        assert_eq!(collector.errors()[0].to_string(), "first error");
        assert_eq!(collector.errors()[1].to_string(), "second error");
    }

    #[test]
    fn stash_errors_can_be_chained_with_other_iterator_methods() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        let results = vec![
            Ok(1),
            Err(oops!("error")),
            Ok(2),
            Ok(3),
            Err(oops!("another error")),
            Ok(4),
        ];

        // Chain with map and filter
        let values: Vec<i32> = results
            .into_iter()
            .stash_errors(&mut collector)
            .map(|x| x * 2)
            .filter(|&x| x > 2)
            .collect();

        assert_eq!(values, vec![4, 6, 8]); // [1*2, 2*2, 3*2, 4*2] filtered for > 2
        assert_eq!(collector.errors().len(), 2);
    }

    // Tests for push_all method
    #[test]
    fn push_all_with_empty_iterator() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        let empty_errors: Vec<AppError> = vec![];

        let result = collector.push_all(empty_errors);

        // push_all should return &mut Self for chaining
        assert!(result.is_empty());
        assert_eq!(result.errors().len(), 0);
    }

    #[test]
    fn push_all_with_single_error() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        let errors = vec![oops!("single error")];

        collector.push_all(errors);

        assert!(!collector.is_empty());
        assert_eq!(collector.errors().len(), 1);
        assert_eq!(collector.errors()[0].to_string(), "single error");
    }

    #[test]
    fn push_all_with_multiple_errors() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        let errors = vec![
            oops!("first error"),
            oops!("second error"),
            oops!("third error"),
        ];

        collector.push_all(errors);

        assert!(!collector.is_empty());
        assert_eq!(collector.errors().len(), 3);
        assert_eq!(collector.errors()[0].to_string(), "first error");
        assert_eq!(collector.errors()[1].to_string(), "second error");
        assert_eq!(collector.errors()[2].to_string(), "third error");
    }

    #[test]
    fn push_all_can_be_chained() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);

        collector
            .push_all(vec![oops!("first"), oops!("second")])
            .push_all(vec![oops!("third")])
            .push(oops!("fourth"));

        assert_eq!(collector.errors().len(), 4);
        assert_eq!(collector.errors()[0].to_string(), "first");
        assert_eq!(collector.errors()[1].to_string(), "second");
        assert_eq!(collector.errors()[2].to_string(), "third");
        assert_eq!(collector.errors()[3].to_string(), "fourth");
    }

    #[test]
    fn push_all_appends_to_existing_errors() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        collector.push(oops!("initial"));

        collector.push_all(vec![oops!("added1"), oops!("added2")]);

        assert_eq!(collector.errors().len(), 3);
        assert_eq!(collector.errors()[0].to_string(), "initial");
        assert_eq!(collector.errors()[1].to_string(), "added1");
        assert_eq!(collector.errors()[2].to_string(), "added2");
    }

    #[test]
    fn push_all_works_with_different_iterables() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);

        // Test with Vec
        collector.push_all(vec![oops!("from vec")]);

        // Test with array
        collector.push_all([oops!("from array")]);

        // Test with iterator chain
        let more_errors = vec![oops!("chained1"), oops!("chained2")];
        collector.push_all(more_errors.into_iter().chain(vec![oops!("chained3")]));

        assert_eq!(collector.errors().len(), 5);
        assert_eq!(collector.errors()[0].to_string(), "from vec");
        assert_eq!(collector.errors()[1].to_string(), "from array");
        assert_eq!(collector.errors()[2].to_string(), "chained1");
        assert_eq!(collector.errors()[3].to_string(), "chained2");
        assert_eq!(collector.errors()[4].to_string(), "chained3");
    }

    #[test]
    fn extend_appends_errors_via_extend_trait() {
        // Use String wrapper to keep test simple
        let mut collector: TypedStash<String, StringErrorWrapper> =
            TypedStash::with_constructor(StringErrorWrapper::new);

        // Start with one error
        collector.push("initial".to_string());

        // Extend with a Vec<String>
        collector.extend(vec!["a".to_string(), "b".to_string()]);

        // Extend with an array of &str converted to String via Into
        collector.extend(["c".to_string(), "d".to_string()]);

        assert_eq!(collector.errors().len(), 5);
        assert_eq!(collector.errors()[0], "initial");
        assert_eq!(collector.errors()[1], "a");
        assert_eq!(collector.errors()[2], "b");
        assert_eq!(collector.errors()[3], "c");
        assert_eq!(collector.errors()[4], "d");
    }

    // Tests for IntoIterator implementation
    #[test]
    fn into_iter_with_empty_stash() {
        let collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);

        let errors: Vec<AppError> = collector.into_iter().collect();

        assert_eq!(errors.len(), 0);
    }

    #[test]
    fn into_iter_with_multiple_errors() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        collector.push(oops!("first"));
        collector.push(oops!("second"));
        collector.push(oops!("third"));

        let errors: Vec<String> = collector.into_iter().map(|e| e.to_string()).collect();

        assert_eq!(errors, vec!["first", "second", "third"]);
    }

    #[test]
    fn into_iter_preserves_order() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        let input_errors = vec![
            oops!("error_1"),
            oops!("error_2"),
            oops!("error_3"),
            oops!("error_4"),
            oops!("error_5"),
        ];

        for error in input_errors {
            collector.push(error);
        }

        let output_errors: Vec<String> = collector.into_iter().map(|e| e.to_string()).collect();

        assert_eq!(
            output_errors,
            vec!["error_1", "error_2", "error_3", "error_4", "error_5"],
        );
    }

    // Test for adding one ErrorStash to another via push_all
    #[test]
    fn push_all_from_another_error_stash() {
        let mut source_collector: TypedStash<_, _> =
            TypedStash::with_constructor(WrapperError::new);
        source_collector.push(oops!("source error 1"));
        source_collector.push(oops!("source error 2"));
        source_collector.push(oops!("source error 3"));

        let mut target_collector: TypedStash<_, _> =
            TypedStash::with_constructor(WrapperError::new);
        target_collector.push(oops!("target initial"));

        // Add all errors from source_collector to target_collector
        target_collector.push_all(source_collector);

        // Verify the target collector contains all errors in the right order
        assert_eq!(target_collector.errors().len(), 4);
        assert_eq!(target_collector.errors()[0].to_string(), "target initial");
        assert_eq!(target_collector.errors()[1].to_string(), "source error 1");
        assert_eq!(target_collector.errors()[2].to_string(), "source error 2");
        assert_eq!(target_collector.errors()[3].to_string(), "source error 3");

        // Verify that we can still create a wrapper error with all collected errors
        let result = target_collector.fail_unless_empty();
        assert!(result.is_err());
        let wrapper_error = result.unwrap_err();
        assert_eq!(wrapper_error.errors.len(), 4);
    }

    #[test]
    fn push_all_multiple_error_stashes() {
        let mut stash1: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        stash1.push(oops!("stash1 error"));

        let mut stash2: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        stash2.push(oops!("stash2 error1"));
        stash2.push(oops!("stash2 error2"));

        let mut stash3: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        stash3.push(oops!("stash3 error"));

        let mut target: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);
        target.push(oops!("target initial"));

        // Chain multiple push_all operations
        target.push_all(stash1).push_all(stash2).push_all(stash3);

        assert_eq!(target.errors().len(), 5);
        assert_eq!(target.errors()[0].to_string(), "target initial");
        assert_eq!(target.errors()[1].to_string(), "stash1 error");
        assert_eq!(target.errors()[2].to_string(), "stash2 error1");
        assert_eq!(target.errors()[3].to_string(), "stash2 error2");
        assert_eq!(target.errors()[4].to_string(), "stash3 error");
    }

    #[test]
    fn check_adds_error_only_when_condition_false() {
        let mut collector: TypedStash<_, _> = TypedStash::with_constructor(WrapperError::new);

        // When condition is true, check should do nothing
        collector.check(true, oops!("should not be added"));
        assert!(collector.is_empty());

        // When condition is false, check should add the error
        collector.check(false, oops!("was added"));
        assert!(!collector.is_empty());
        assert_eq!(collector.errors().len(), 1);
        assert_eq!(collector.errors()[0].to_string(), "was added");

        // Calling fail_unless_empty should return Err and consume the stash
        let res = collector.fail_unless_empty();
        assert!(res.is_err());
        let wrapper = res.unwrap_err();
        assert_eq!(wrapper.errors.len(), 1);
        assert!(collector.is_empty());
    }

    #[test]
    fn new_initializes_with_empty_errors() {
        // Use String as the child error type so the produced ErrorList<String>
        // is easy to inspect.
        let stash: TypedStash<String, ErrorList<String>> = TypedStash::new();

        // No errors should be present initially
        assert!(stash.is_empty());

        // Converting to Option<ErrorList<String>> should return None since
        // there are no collected errors.
        let maybe_err: Option<ErrorList<String>> = stash.into();
        assert!(maybe_err.is_none());
    }

    #[test]
    fn new_initializes_with_default_summary() {
        // Create a new stash with default summary
        let errlist = TypedStash::new()
            .push("error one")
            .push("error two")
            .to_error()
            .unwrap();

        // to_error should return Some(ErrorList) and its summary() should
        // include the default summary with count substituted (2)
        let summary = errlist.summary();
        assert_eq!(summary, "Encountered 2 errors:");
        assert_eq!(
            "Encountered 2 errors:\n- error one\n- error two\n",
            errlist.to_string()
        );
    }

    #[test]
    fn with_summary_uses_provided_summary_and_counts_errors() {
        // Create a stash with a custom summary string.
        let errlist = TypedStash::with_summary("Found {count} hiccups:")
            .push("one")
            .push("two")
            .to_error()
            .unwrap();

        let summary = errlist.summary();
        assert_eq!(summary, "Found 2 hiccups:");
        assert!(errlist.to_string().starts_with("Found 2 hiccups:\n"));
    }

    #[test]
    fn thiserr_child_error() {
        #[derive(thiserror::Error, Debug)]
        #[error("Child error: {0}")]
        struct ChildError(String);

        let child = ChildError("something went wrong".to_string());

        let wrapper = TypedStash::new().push(child).to_error().unwrap();
        assert_eq!(wrapper.len(), 1);
        assert_eq!(wrapper[0].to_string(), "Child error: something went wrong");
    }

    #[test]
    fn thiserror_wrapper_error() {
        #[derive(thiserror::Error, Debug)]
        #[error("Wrapper for {} errors (first error: {:?})", .0.len(), .0.first())]
        struct Wrapper(Vec<String>);

        impl Wrapper {
            fn new(errors: Vec<String>) -> Self {
                Self(errors)
            }
        }

        let mut stash: TypedStash<String, Wrapper> = TypedStash::with_constructor(Wrapper::new);
        stash.push("first issue".to_string());
        stash.push("second issue".to_string());

        let wrapper = stash.to_error().unwrap();
        assert_eq!(wrapper.0.len(), 2);
        let first_error = &wrapper.0[0];
        assert!(first_error.contains("first issue"));
    }

    #[test]
    fn anyhow_child_error() {
        // Use anyhow::Error as the child error type for TypedStash
        let mut stash: TypedStash<anyhow::Error, ErrorList<anyhow::Error>> = TypedStash::new();

        stash.push(anyhow::anyhow!("downstream failure"));

        let errlist = stash.to_error().unwrap();
        assert_eq!(errlist.len(), 1);
        // Ensure the error message is preserved
        let msgs: Vec<String> = errlist.into_iter().map(|e| e.to_string()).collect();
        assert_eq!(msgs, vec!["downstream failure"]);
    }

    #[test]
    fn anyhow_wraps_errorlist_and_downcast() {
        // Create an ErrorList of Strings and wrap it into an anyhow::Error
        let errlist: ErrorList<String> = TypedStash::with_summary("Found {count} problems:")
            .push("a".to_string())
            .push("b".to_string())
            .to_error()
            .unwrap();

        // Convert the ErrorList into an anyhow::Error
        let any_err: anyhow::Error = errlist.into();

        // Borrow the inner ErrorList via downcast_ref to inspect it without
        // moving it out of the anyhow::Error (avoids size/coercion issues).
        let recovered_ref = (any_err).downcast_ref::<ErrorList<String>>();
        assert!(recovered_ref.is_some());
        let recovered = recovered_ref.unwrap();
        assert_eq!(recovered.len(), 2);
        assert_eq!(recovered[0], "a");
        assert_eq!(recovered[1], "b");
    }

    #[test]
    fn eyre_child_error() {
        // Use eyre::Report as the child error type for TypedStash
        let mut stash: TypedStash<eyre::Report, ErrorList<eyre::Report>> = TypedStash::new();

        stash.push(eyre::eyre!("downstream failure"));

        let errlist = stash.to_error().unwrap();
        assert_eq!(errlist.len(), 1);
        // Ensure the error message is preserved
        let msgs: Vec<String> = errlist.into_iter().map(|e| e.to_string()).collect();
        assert_eq!(msgs, vec!["downstream failure"]);
    }

    #[test]
    fn eyre_wraps_errorlist_and_downcast() {
        // Create an ErrorList of Strings and wrap it into an eyre::Report
        let errlist: ErrorList<String> = TypedStash::with_summary("Found {count} problems:")
            .push("a".to_string())
            .push("b".to_string())
            .to_error()
            .unwrap();

        // Convert the ErrorList into an eyre::Report
        let report: eyre::Report = errlist.into();

        // Borrow the inner ErrorList via downcast_ref to inspect it without
        // moving it out of the eyre::Report.
        let recovered_ref = (report).downcast_ref::<ErrorList<String>>();
        assert!(recovered_ref.is_some());
        let recovered = recovered_ref.unwrap();
        assert_eq!(recovered.len(), 2);
        assert_eq!(recovered[0], "a");
        assert_eq!(recovered[1], "b");
    }
}
