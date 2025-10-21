use std::error::Error;
use std::fmt::{Debug, Display};

/// An internal trait for methods that default methods in the main
/// trait need to call, but that we don't want to expose publicly.
#[doc(hidden)]
pub trait ErrorStashInternal<E, W>: Debug
where
    E: Debug + Display + 'static,
    W: Send + 'static,
{
    /// If no errors have been collected, calls the closure and returns
    /// `Ok(result)`. Otherwise, consumes the collected errors and returns
    /// `Err(W)`.
    ///
    /// This is similar to the public `to_result` method, but trait default
    /// methods can't take `self` by value (they can't know the size of
    /// non-reference value), so this method implements most of the logic, and
    /// the specific trait implementations can call it while consuming `self`.
    fn to_result<T>(&mut self, closure: impl FnOnce() -> T) -> Result<T, W> {
        if self.errors().is_empty() {
            Ok(closure())
        } else {
            let wrapper = self.consume();
            debug_assert!(self.errors().is_empty());
            Err(wrapper)
        }
    }

    /// If no errors have been collected, returns `None`. Otherwise,
    /// consumes the collected errors and returns `Some(W)`.
    ///
    /// Like `to_result`, this is similar to the public `to_error` method,
    /// and is provided here because trait default methods can't take `self`
    /// by value.
    fn to_error(&mut self) -> Option<W> {
        if self.errors().is_empty() {
            None
        } else {
            let wrapper = self.consume();
            debug_assert!(self.errors().is_empty());
            Some(wrapper)
        }
    }

    /// Returns a slice of the collected errors.
    fn errors(&self) -> &[E];

    /// Returns a mutable reference to the collected errors.
    fn mut_errors(&mut self) -> &mut Vec<E>;

    /// Consumes the stash and returns a wrapper error containing all
    /// collected errors.
    fn consume(&mut self) -> W;
}

/// Collects child errors, then emits a wrapper error if any were collected.
///
/// ErrorStashes are useful when validating data where multiple independent
/// errors may occur, and you want to report all of them together rather than
/// failing fast on the first error.
///
/// `E` is the child error type that implements `std::fmt::Debug`.
/// `W` is the wrapper error type that implements `std::error::Error`.
///
/// ## Terminal methods
///
/// ErrorStash methods that can return the wrapper error type `W` are called
/// "terminal methods" because callers are expected to call them with the `?`
/// operator and return from the calling function if any errors were collected.
/// For example, [`fail_unless_empty`][ErrorStash::fail_unless_empty] will return
/// early from the calling function if any errors were collected (and continue
/// execution otherwise).
///
/// The terminal methods of error stashes are:
/// - [`to_result`][ErrorStash::to_result]
/// - [`to_error`][ErrorStash::to_error]
/// - [`fail_now`][ErrorStash::fail_now]
/// - [`fail_unless_empty`][ErrorStash::fail_unless_empty]
///
/// These methods will reset the stash to an empty state if any errors were
/// collected prior to the call. This generally doesn't matter if they're called
/// with `?` because the calling function will return and the stash will no
/// longer be needed. However, if you call a terminal method without `?`, be
/// aware that following the call the stash will be empty- if it is a stash type
/// that takes additional configuration, like the summary message in
/// [`BoxedStash`], that configuration will also be reset to its default state.
///
pub trait ErrorStash<E, W>: ErrorStashInternal<E, W>
where
    E: Debug + Display + 'static,
    W: Send + 'static,
{
    // Provided methods

    /// Returns true if no errors have been collected.
    ///
    /// ```
    /// # use errorstash::{ErrorStash, BoxedStash};
    ///
    /// let mut stash = BoxedStash::new();
    /// assert!(stash.is_empty());
    /// stash.push("an error");
    /// assert!(!stash.is_empty());
    /// ```
    fn is_empty(&self) -> bool {
        self.errors().is_empty()
    }

    /// Returns the number of collected errors.
    ///
    /// ```
    /// # use errorstash::{ErrorStash, BoxedStash};
    ///
    /// let mut stash = BoxedStash::new();
    /// assert_eq!(stash.len(), 0);
    /// stash.push("an error");
    /// stash.push("another error");
    /// assert_eq!(stash.len(), 2);
    /// ```
    fn len(&self) -> usize {
        self.errors().len()
    }

    /// If no errors have been collected, returns `Some(())`. Otherwise,
    /// returns `None`.
    ///
    /// This method is designed for use with the `?` operator in functions that
    /// return `Option<T>`, such as functions that have their ErrorStash passed
    /// into them and don't return their errors directly. It can be called at
    /// any time with `?` to return from the function if any errors have been
    /// collected.
    fn ok(&self) -> Option<()> {
        if self.is_empty() {
            Some(())
        } else {
            None
        }
    }

    /// If the condition is false, adds a formatted error to the stash. Otherwise,
    /// does nothing.
    ///
    /// This method is like [`check`][crate::BoxedStash::check], but allows you
    /// to provide a dynamically generated error value via the [`format_args!`]
    /// macro, as long as this stash's error type implements `From<String>`.
    /// This is always true for [`BoxedStash`][crate::BoxedStash], but for
    /// [`TypedStash`][crate::TypedStash] it depends on the error type used.
    ///
    /// For performance, the formatting is only evaluated if the condition is
    /// false.
    ///
    /// # Example
    /// ```
    /// # use errorstash::{ErrorStash, BoxedStash};
    ///
    /// let mut stash = BoxedStash::new();
    /// let value = 42;
    /// stash.check_fmt(value > 100, format_args!("value {} is not greater than 100", value));
    /// assert_eq!(stash.len(), 1);
    /// // `errors()` is not part of the public trait surface, so consume the stash
    /// // and inspect the returned wrapper to check the formatted message.
    /// let wrapper = stash.to_error().unwrap();
    /// let vec = wrapper.to_vec();
    /// assert_eq!(vec[0].to_string(), "value 42 is not greater than 100");
    /// ```
    ///
    /// If you want to return immediately if the condition is false,
    /// chain a call to [`fail_unless_empty`] after this method. For example:
    ///
    /// ```ignore
    /// let mut stash = BoxedStash::new();
    /// let value = 42;
    /// stash.check_fmt(value > 100, format_args!("value {} is not greater than 100", value))
    ///      .fail_unless_empty()?;
    /// ```
    fn check_fmt(&mut self, condition: bool, args: std::fmt::Arguments<'_>) -> &mut Self
    where
        E: From<String>,
    {
        if !condition {
            self.mut_errors().push(E::from(args.to_string()));
        }
        self
    }

    /// If no errors have been collected, returns `Ok(())`. Otherwise, consumes
    /// the collected errors and returns `Err(W)`.
    ///
    /// Typically, you will call this method with the `?` operator, returning
    /// early from the current function if any errors were collected,
    /// and continuing execution otherwise. If you call this method without `?`,
    /// be aware that this stash's collected errors will be consumed and
    /// it will always be empty after this call.
    fn fail_unless_empty(&mut self) -> Result<(), W> {
        if self.is_empty() {
            return Ok(());
        }

        let wrapper = self.consume();
        Err(wrapper)
    }

    // Required methods

    /// If no errors have been collected, calls the closure and returns `Ok(result)`. Otherwise,
    /// consumes the collected errors and returns `Err(W)`.
    fn to_result<T>(self, closure: impl FnOnce() -> T) -> Result<T, W>;

    /// If no errors have been collected, returns `None`. Otherwise,
    /// consumes the collected errors and returns `Some(W)`.
    fn to_error(self) -> Option<W>;
}

/// An extension trait for [Result] that adds
/// [or_stash][StashableResult::or_stash] and
/// [or_fail][StashableResult::or_fail] methods that can be used to stash
/// errors.
pub trait StashableResult<T, E, W, S>
where
    E: Display + Debug + Send + 'static,
    W: Send + 'static,
    S: ErrorStash<E, W>,
{
    /// Consumes this `Result`, returning `Some(T)` if this result is ok, or
    /// collecting the error into the provided `ErrorStash` and returning `None`
    /// if this result is an error.
    ///
    /// This method can be used when you want to stash the result's error but
    /// continue execution. It will return an empty `Option` if an error
    /// occurred, so you should not attempt to unwrap the result until after
    /// checking the stash for errors.
    ///
    /// ## Example
    /// ```
    /// # use errorstash::{ErrorStash, StashableResult, BoxedStash};
    /// use anyhow::{Error, anyhow};
    ///
    /// let mut stash = BoxedStash::new();
    /// let r1: Result<i32, Error> = Ok(1);
    /// let r2: Result<i32, Error> = Err(anyhow!("error"));
    ///
    /// let v1 = r1.or_stash(&mut stash);
    /// let v2 = r2.or_stash(&mut stash);
    ///
    /// assert_eq!(v1, Some(1));
    /// assert_eq!(v2, None);
    /// assert_eq!(stash.len(), 1);
    /// ```
    ///
    /// # Use with iterators
    ///
    /// This method can also be used with the [filter_map][Iterator::filter_map]
    /// method of iterators to transform an iterator of `Result<T, E>` into an
    /// iterator of `T`, stashing any errors along the way. Note that the
    /// [`StashErrorsIter`] trait does the same thing via a method on the
    /// `Iterator` rather than on each `Result`.
    ///
    /// ## Example
    /// ```
    /// # use errorstash::{ErrorStash, StashableResult, BoxedStash};
    /// use anyhow::{Error, anyhow};
    ///
    /// let mut stash = BoxedStash::new();
    /// let results = vec![Ok(1), Err(anyhow!("error")), Ok(2)];
    /// let values: Vec<i32> = results.into_iter()
    ///     .filter_map(|r| r.or_stash(&mut stash))
    ///     .collect();
    ///
    /// assert_eq!(values, vec![1, 2]);
    /// assert_eq!(stash.len(), 1);
    /// ```
    fn or_stash(self, stash: &mut S) -> Option<T>;

    /// Consumes this `Result`, returning `Ok(T)` if this result is ok, or
    /// collecting the error into the provided `ErrorStash` and returning the
    /// aggregated errors in a `Err(W)` if this result is an error.
    ///
    /// This changes the result's error from containing just the original error
    /// to containing all errors collected in the stash (including this one). It
    /// can be used when you want to return immediately if the previous
    /// operation failed, but want to include all previously collected errors in
    /// the error result as well.
    ///
    /// Note that if this result is `Ok`, but the stash already contains errors,
    /// this method will still return `Ok(T)`. To return `Err(W)` if any errors
    /// have been collected, call
    /// [`fail_unless_empty`][ErrorStash::fail_unless_empty] on the stash after
    /// calling this method.
    ///
    /// ## Example
    /// ```
    /// # use errorstash::{ErrorStash, StashableResult, BoxedStash};
    /// use anyhow::{Error, anyhow};
    ///
    /// let mut stash = BoxedStash::new();
    /// stash.push(anyhow!("previous error"));
    ///
    /// let r1: Result<i32, Error> = Ok(1);
    /// let r2: Result<i32, Error> = Err(anyhow!("error"));
    ///
    /// let v1 = r1.or_fail(&mut stash);
    /// let v2 = r2.or_fail(&mut stash);
    ///
    /// assert!(v1.is_ok());
    /// assert_eq!(v1.unwrap(), 1);
    ///
    /// assert!(v2.is_err());
    /// let vec = v2.unwrap_err().to_vec();
    /// assert_eq!(vec[0].to_string(), "previous error");
    /// assert_eq!(vec[1].to_string(), "error");
    /// ```
    fn or_fail(self, stash: &mut S) -> Result<T, W>;
}

/// Adds a `stash_errors` method to iterators of `Result<T, E>`, allowing errors
/// to be collected into an `ErrorStash` while producing an iterator of unwrapped values.
pub trait StashErrorsIter<T, W, E, S>
where
    E: Display + Debug + Send + 'static,
    W: Send + 'static,
    S: ErrorStash<E, W>,
{
    /// Converts an iterator of `Result<T, E>` into an iterator of `T`,
    /// collecting all errors into the provided `ErrorStash`.
    ///
    /// # Arguments
    /// * `stash` - A mutable reference to an `ErrorStash` where errors will be collected
    ///
    /// # Returns
    /// An iterator that yields only the unwrapped `Ok` values from the original iterator.
    /// All `Err` values are collected in the provided `ErrorStash`.
    ///
    /// **Note:** The returned iterator borrows `stash` mutably for its entire lifetime.
    ///
    /// # Example
    /// ```
    /// use errorstash::{ErrorStash, StashErrorsIter, BoxedStash};
    /// use anyhow::{Error, anyhow};
    ///
    /// let mut stash = BoxedStash::new();
    /// let results = vec![Ok(1), Err(anyhow!("error")), Ok(2)];
    /// let values: Vec<i32> = results.into_iter().stash_errors(&mut stash).collect();
    ///
    /// assert_eq!(values, vec![1, 2]);
    /// assert_eq!(stash.len(), 1);
    /// ```
    fn stash_errors(self, stash: &mut S) -> impl Iterator<Item = T>;
}

/// Implements `StashErrorsIter` for iterators over `Result<T, E>`, allowing errors to be stashed and values to be collected.
impl<I, T, W, E, FE, S> StashErrorsIter<T, W, E, S> for I
where
    I: Iterator<Item = Result<T, FE>>,
    FE: Into<E>,
    E: Display + Debug + Send + 'static,
    W: Error + Send + 'static,
    S: ErrorStash<E, W>,
{
    fn stash_errors(self, stash: &mut S) -> impl Iterator<Item = T> {
        self.filter_map(move |result| match result {
            Ok(value) => Some(value),
            Err(e) => {
                stash.mut_errors().push(e.into());
                None
            }
        })
    }
}
