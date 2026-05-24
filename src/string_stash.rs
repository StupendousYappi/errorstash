use std::borrow::Cow;
use std::fmt::{Debug, Display};

use crate::error_list::{ErrorList, ErrorSummary};
use crate::error_stash::ErrorStashInternal;
use crate::{ErrorStash, StashableResult};

/// A simple wrapper around [`Cow<str>`] that implements [`std::error::Error`].
///
/// This type allows `StringStash` to collect errors that implement the `Error` trait,
/// while maintaining string-based error handling.
#[derive(Clone, PartialEq, Eq)]
pub struct StringError(Cow<'static, str>);

impl std::ops::Deref for StringError {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for StringError {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl Debug for StringError {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl std::error::Error for StringError {}

impl From<String> for StringError {
    #[inline]
    fn from(s: String) -> Self {
        Self(Cow::Owned(s))
    }
}

impl From<&'static str> for StringError {
    #[inline]
    fn from(s: &'static str) -> Self {
        Self(Cow::Borrowed(s))
    }
}

impl From<Cow<'static, str>> for StringError {
    #[inline]
    fn from(s: Cow<'static, str>) -> Self {
        Self(s)
    }
}

impl From<StringError> for String {
    #[inline]
    fn from(err: StringError) -> Self {
        err.0.into_owned()
    }
}

impl PartialEq<str> for StringError {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for StringError {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl StringError {
    /// Creates a new `StringError` from any type that implements `Display + 'static`.
    ///
    /// This uses zero-cost type reflection to avoid copies for `&'static str`
    /// and clones for `String`.
    pub fn from<T: Display + 'static>(err: T) -> Self {
        use std::any::TypeId;
        use std::mem::ManuallyDrop;

        let err = ManuallyDrop::new(err);
        let type_id = TypeId::of::<T>();

        if type_id == TypeId::of::<&'static str>() {
            let s: &'static str = unsafe { std::ptr::read(&*err as *const _ as *const &'static str) };
            Self(Cow::Borrowed(s))
        } else if type_id == TypeId::of::<String>() {
            let s: String = unsafe { std::ptr::read(&*err as *const _ as *const String) };
            Self(Cow::Owned(s))
        } else if type_id == TypeId::of::<Cow<'static, str>>() {
            let s: Cow<'static, str> = unsafe { std::ptr::read(&*err as *const _ as *const Cow<'static, str>) };
            Self(s)
        } else if type_id == TypeId::of::<Self>() {
            let s: Self = unsafe { std::ptr::read(&*err as *const _ as *const Self) };
            s
        } else {
            let s = unsafe { std::ptr::read(&*err as *const T) };
            Self(Cow::Owned(s.to_string()))
        }
    }
}

/// An [`ErrorStash`] that accepts anything that implements [`Display`]
/// and stores it as a [`StringError`] containing its display text.
///
/// It produces an [`ErrorList<StringError>`] if any errors are collected.
///
/// `StringStash` is useful when you want to collect arbitrary error messages
/// or formatted text as strings, without needing them to implement the `Error` trait,
/// or when you want to simplify string-based error handling.
///
/// # Terminal methods
///
/// Methods that can return an [`ErrorList<StringError>`] (i.e. inside a `Result` or
/// `Option`) are considered terminal methods, as they consume the collected
/// errors to produce the wrapper error. In `StringStash`, these methods not only
/// consume all collected errors, they also reset the stash's summary line to
/// its default value.
///
/// Typically, this doesn't matter, because these methods
/// are normally called with the `?` operator, causing the calling function to
/// return and the stash to likely no longer be used if any errors ocurred. However,
/// if you do call a terminal method without propagating the error immediately
/// (i.e. without using `?`), be aware after the call the stash will be empty
/// and the summary line reset.
///
/// The terminal methods on `StringStash` are:
/// - [`StringStash::to_result`]
/// - [`StringStash::to_error`]
/// - [`StringStash::fail_now`]
/// - [`StringStash::fail_unless_empty`]
#[derive(Debug, Default)]
pub struct StringStash {
    summary: ErrorSummary,
    errors: Vec<StringError>,
}

impl ErrorStashInternal<StringError, ErrorList<StringError>> for StringStash {
    fn errors(&self) -> &[StringError] {
        &self.errors
    }

    fn mut_errors(&mut self) -> &mut Vec<StringError> {
        &mut self.errors
    }

    fn consume(&mut self) -> ErrorList<StringError> {
        let errors = std::mem::take(&mut self.errors);
        let summary = std::mem::take(&mut self.summary).with_count(errors.len());
        ErrorList::new(summary, errors)
    }
}

impl ErrorStash<StringError, ErrorList<StringError>> for StringStash {
    fn to_result<T>(mut self, closure: impl FnOnce() -> T) -> Result<T, ErrorList<StringError>> {
        ErrorStashInternal::to_result(&mut self, closure)
    }

    fn to_error(mut self) -> Option<ErrorList<StringError>> {
        ErrorStashInternal::to_error(&mut self)
    }
}

impl StringStash {
    /// Creates a new StringStash with the default summary line.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new StringStash with the given summary line.
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
    /// so that the resulting [`StringStash`] remains thread-safe.
    pub fn set_summary_with(
        &mut self,
        summary_func: impl Fn() -> String + Send + Sync + 'static,
    ) -> &mut Self {
        let summary = ErrorSummary::new_dynamic(summary_func);
        self.summary = summary;
        self
    }

    /// Adds a child error to the stash, converting it to a StringError.
    pub fn push(&mut self, err: impl Display + 'static) -> &mut Self {
        self.mut_errors().push(StringError::from(err));
        self
    }

    /// Adds multiple child errors to the stash, converting them to StringErrors.
    pub fn push_all<T, It>(&mut self, errors: It) -> &mut Self
    where
        It: IntoIterator<Item = T>,
        T: Display + 'static,
    {
        self.mut_errors()
            .extend(errors.into_iter().map(|e| StringError::from(e)));
        self
    }

    /// If the condition is false, adds error `e` to the stash. Otherwise,
    /// does nothing.
    ///
    /// If you want to return immediately if the condition is false,
    /// chain a call to [`fail_unless_empty`][ErrorStash::fail_unless_empty] after this method.
    pub fn check(&mut self, condition: bool, e: impl Display + 'static) -> &mut Self {
        if !condition {
            self.mut_errors().push(StringError::from(e));
        }
        self
    }

    /// Adds an error and immediately returns `Err(ErrorList<StringError>)` with all collected
    /// errors.
    pub fn fail_now(&mut self, e: impl Display + 'static) -> Result<(), ErrorList<StringError>> {
        self.mut_errors().push(StringError::from(e));
        let wrapper = self.consume();
        Err(wrapper)
    }
}

impl Display for StringStash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StringStash with {} errors", self.errors.len())
    }
}

impl<T> Extend<T> for StringStash
where
    T: Display + 'static,
{
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.errors
            .extend(iter.into_iter().map(|t| StringError::from(t)));
    }
}

impl IntoIterator for StringStash {
    type Item = StringError;
    type IntoIter = std::vec::IntoIter<StringError>;

    fn into_iter(self) -> Self::IntoIter {
        self.errors.into_iter()
    }
}

use crate::boxed_stash::BoxedError;
use std::any::{Any, TypeId};

/// Allows stashing errors from ANY result error type that implements `std::fmt::Display` into a `StringStash`.
///
/// If the error type is an `ErrorList<StringError>` or `ErrorList<BoxedError>`, it will be unpacked
/// and its individual child errors will be stashed separately as `StringError`s, maintaining proper
/// multi-error formatting.
impl<T, FE> StashableResult<T, StringError, ErrorList<StringError>, StringStash> for Result<T, FE>
where
    FE: Display + Send + Any + 'static,
{
    fn or_stash(self, stash: &mut StringStash) -> Option<T> {
        match self {
            Ok(v) => Some(v),
            Err(e) => {
                let any_ref = &e as &dyn Any;
                let error_list_id = TypeId::of::<ErrorList<StringError>>();
                let boxed_error_list_id = TypeId::of::<ErrorList<BoxedError>>();

                if any_ref.type_id() == error_list_id {
                    let wrapper = any_ref
                        .downcast_ref::<ErrorList<StringError>>()
                        .expect("TypeId matched but downcast failed");
                    stash.mut_errors().extend(wrapper.clone());
                } else if any_ref.type_id() == boxed_error_list_id {
                    let wrapper = any_ref
                        .downcast_ref::<ErrorList<BoxedError>>()
                        .expect("TypeId matched but downcast failed");
                    stash
                        .mut_errors()
                        .extend(wrapper.iter().map(|err| StringError::from(err.to_string())));
                } else {
                    stash.mut_errors().push(StringError::from(e));
                }
                None
            }
        }
    }

    fn or_fail(self, stash: &mut StringStash) -> Result<T, ErrorList<StringError>> {
        self.map_err(|e| {
            let any_ref = &e as &dyn Any;
            let error_list_id = TypeId::of::<ErrorList<StringError>>();
            let boxed_error_list_id = TypeId::of::<ErrorList<BoxedError>>();

            if any_ref.type_id() == error_list_id {
                let wrapper = any_ref
                    .downcast_ref::<ErrorList<StringError>>()
                    .expect("TypeId matched but downcast failed");
                stash.mut_errors().extend(wrapper.clone());
            } else if any_ref.type_id() == boxed_error_list_id {
                let wrapper = any_ref
                    .downcast_ref::<ErrorList<BoxedError>>()
                    .expect("TypeId matched but downcast failed");
                stash
                    .mut_errors()
                    .extend(wrapper.iter().map(|err| StringError::from(err.to_string())));
            } else {
                stash.mut_errors().push(StringError::from(e));
            }
            stash.consume()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StashableResult;
    use test_log::test;

    #[test]
    fn basic_use() {
        let mut stash = StringStash::new();
        stash.set_summary("Found {count} errors");
        stash.push("first error");
        stash.push("Second error".to_string());

        let errors = stash.fail_now(42).unwrap_err();
        assert_eq!(3, errors.len());
        assert_eq!("first error", &*errors[0]);
        assert_eq!("Second error", &*errors[1]);
        assert_eq!("42", &*errors[2]);
        assert_eq!("Found 3 errors", errors.summary());
    }

    #[test]
    fn display_with_count_replacement() {
        let mut stash = StringStash::with_summary("Multiple errors occurred: {count} issues found");
        stash.push("IO error occurred");
        stash.push("Custom error occurred");

        let error_list = stash.to_error().unwrap();
        let display_output = format!("{}", error_list);
        let expected_output = "Multiple errors occurred: 2 issues found\n- IO error occurred\n- Custom error occurred\n";
        assert_eq!(expected_output, display_output);
    }

    #[test]
    fn push_all_with_strs() {
        let strs = ["error one", "error two", "error three"];
        let mut stash = StringStash::new();
        stash.push_all(strs.iter().copied());
        let errors = stash.consume();
        assert_eq!(errors.len(), 3);
        assert_eq!("error one", &*errors[0]);
        assert_eq!("error two", &*errors[1]);
        assert_eq!("error three", &*errors[2]);
    }

    #[test]
    fn summary_with_capturing_closure() {
        let prefix = "errors detected";
        let mut stash = StringStash::new();
        stash.set_summary_with(move || prefix.to_ascii_uppercase() + ": {count}");
        stash.push("boom");

        let errors = stash.consume();
        assert_eq!(1, errors.len());
        assert_eq!("ERRORS DETECTED: 1", errors.summary());
    }

    #[test]
    fn check_adds_error_when_false() {
        let mut stash = StringStash::new();
        stash.check(false, "should be added");
        stash.check(true, "should not be added");
        assert_eq!(1, stash.len());
        assert_eq!("should be added", &*stash.consume()[0]);
    }

    #[test]
    fn extend_and_into_iterator() {
        let mut stash = StringStash::new();
        stash.extend(vec!["error 1", "error 2"]);
        assert_eq!(stash.len(), 2);
        let collected: Vec<StringError> = stash.into_iter().collect();
        assert_eq!(
            collected,
            vec![
                StringError::from("error 1".to_string()),
                StringError::from("error 2".to_string())
            ]
        );
    }

    #[test]
    fn or_stash_simple_error() {
        let mut stash = StringStash::new();
        let result: Result<i32, &str> = Err("an error occurred");
        let value = result.or_stash(&mut stash);
        assert!(value.is_none());
        assert_eq!(1, stash.len());
        assert_eq!("an error occurred", &*stash.consume()[0]);
    }

    #[test]
    fn or_stash_with_string_error() {
        let mut stash = StringStash::new();
        let result: Result<i32, StringError> = Err(StringError::from("string error".to_string()));
        let value = result.or_stash(&mut stash);
        assert!(value.is_none());
        assert_eq!(1, stash.len());
        assert_eq!("string error", &*stash.consume()[0]);
    }

    #[test]
    fn or_stash_with_errorlist() {
        let mut target_stash = StringStash::new();
        let source_errors = ErrorList::new(
            "summary".into(),
            vec![
                StringError::from("error A".to_string()),
                StringError::from("error B".to_string()),
            ],
        );
        let err_value: Result<i32, ErrorList<StringError>> = Err(source_errors);

        let value = err_value.or_stash(&mut target_stash);
        assert!(value.is_none());
        assert_eq!(2, target_stash.len());
        let errors = target_stash.consume();
        assert_eq!("error A", &*errors[0]);
        assert_eq!("error B", &*errors[1]);
    }

    #[test]
    fn or_stash_with_anyhow_error() {
        let mut stash = StringStash::new();
        let result: Result<i32, anyhow::Error> = Err(anyhow::anyhow!("anyhow error message"));
        let value = result.or_stash(&mut stash);
        assert!(value.is_none());
        assert_eq!(1, stash.len());
        assert_eq!("anyhow error message", &*stash.consume()[0]);
    }

    #[test]
    fn or_stash_with_boxed_error_list() {
        let mut target_stash = StringStash::new();
        let mut source_stash = crate::BoxedStash::new();
        source_stash.push("boxed err 1");
        source_stash.push("boxed err 2");
        let source_errors = source_stash.to_error().unwrap();
        let err_value: Result<i32, ErrorList<BoxedError>> = Err(source_errors);

        let value = err_value.or_stash(&mut target_stash);
        assert!(value.is_none());
        assert_eq!(2, target_stash.len());
        let errors = target_stash.consume();
        assert_eq!("boxed err 1", &*errors[0]);
        assert_eq!("boxed err 2", &*errors[1]);
    }

    #[test]
    fn empty_stash_behavior() {
        let stash = StringStash::new();
        assert!(stash.is_empty());
        assert_eq!(stash.len(), 0);

        // Attempting to produce an ErrorList from an empty stash should yield None
        let error = stash.to_error();
        assert!(error.is_none());

        let stash2 = StringStash::new();
        // to_result on an empty stash should return Ok(val)
        let result = stash2.to_result(|| "success");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");

        let mut stash3 = StringStash::new();
        // fail_unless_empty on an empty stash should return Ok(())
        let res = stash3.fail_unless_empty();
        assert!(res.is_ok());
    }

    #[test]
    fn string_error_reflection_optimization() {
        use std::borrow::Cow;

        let literal = "test error literal";
        let err_literal = StringError::from(literal);
        assert!(matches!(err_literal.0, Cow::Borrowed(_)));
        assert_eq!(&*err_literal, literal);

        let owned = "test error owned".to_string();
        let err_owned = StringError::from(owned);
        assert!(matches!(err_owned.0, Cow::Owned(_)));
        assert_eq!(&*err_owned, "test error owned");

        let cow_borrowed: Cow<'static, str> = Cow::Borrowed("cow borrowed");
        let err_cow_borrowed = StringError::from(cow_borrowed);
        assert!(matches!(err_cow_borrowed.0, Cow::Borrowed(_)));
        assert_eq!(&*err_cow_borrowed, "cow borrowed");

        let self_err = StringError::from(StringError::from("nested"));
        assert!(matches!(self_err.0, Cow::Borrowed(_)));
        assert_eq!(&*self_err, "nested");
    }
}
