use std::{
    borrow::Cow,
    error::Error,
    fmt::{Debug, Display, Formatter, Write},
    ops::Deref,
};

const COUNT_PLACEHOLDER: &str = "{count}";

pub(crate) const DEFAULT_SUMMARY: &str = "Encountered {count} errors:";

/// A summary message for a collection of errors.
///
/// This can be either a static string or a closure that generates a string. Either
/// version can include the placeholder `{count}`, which will be replaced with the
/// number of errors when formatted.
pub(crate) enum ErrorSummary {
    Static(Cow<'static, str>, usize),
    Dynamic(Box<dyn Fn() -> String + Send + Sync + 'static>, usize),
}

impl Debug for ErrorSummary {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorSummary::Static(summary, count) => {
                f.debug_tuple("Static").field(summary).field(count).finish()
            }
            ErrorSummary::Dynamic(_, count) => f
                .debug_tuple("Dynamic")
                .field(&"<closure>")
                .field(count)
                .finish(),
        }
    }
}

impl Default for ErrorSummary {
    fn default() -> Self {
        ErrorSummary::new_static(DEFAULT_SUMMARY)
    }
}

impl ErrorSummary {
    pub(crate) const fn new_static(summary: &'static str) -> Self {
        Self::Static(Cow::Borrowed(summary), 0)
    }

    pub(crate) fn new_dynamic(summary_func: impl Fn() -> String + Send + Sync + 'static) -> Self {
        Self::Dynamic(Box::new(summary_func), 0)
    }

    fn count(&self) -> usize {
        match self {
            ErrorSummary::Static(_, count) => *count,
            ErrorSummary::Dynamic(_, count) => *count,
        }
    }

    fn as_cow(&self) -> Cow<'static, str> {
        match self {
            ErrorSummary::Static(s, _) => s.clone(),
            ErrorSummary::Dynamic(func, _) => Cow::Owned(func()),
        }
    }

    fn freeze(&self) -> Self {
        match self {
            ErrorSummary::Static(s, count) => ErrorSummary::Static(s.clone(), *count),
            ErrorSummary::Dynamic(f, count) => {
                let s = (f)();
                ErrorSummary::Static(Cow::Owned(s), *count)
            }
        }
    }

    pub(crate) fn with_count(self, count: usize) -> Self {
        match self {
            ErrorSummary::Static(s, _) => ErrorSummary::Static(s, count),
            ErrorSummary::Dynamic(f, _) => ErrorSummary::Dynamic(f, count),
        }
    }
}

impl Display for ErrorSummary {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let count = self.count();
        let as_cow = self.as_cow();
        let raw_summary = as_cow.as_ref();
        // Find the placeholder "{count}" in the summary
        let count_loc = raw_summary
            .find(COUNT_PLACEHOLDER)
            .map(|i| (i, i + COUNT_PLACEHOLDER.len()));
        if let Some((start, end)) = count_loc {
            f.write_str(&raw_summary[..start])?;
            write!(f, "{}", count)?;
            f.write_str(&raw_summary[end..])?;
        } else {
            // If "{count}" is not found, just write the summary as is
            f.write_str(raw_summary)?;
        }
        Ok(())
    }
}

impl From<&'static str> for ErrorSummary {
    fn from(s: &'static str) -> Self {
        ErrorSummary::new_static(s)
    }
}

/// A heterogeneous collection of errors, along with a summary message.
pub struct ErrorList<T>
where
    T: Display + Debug,
{
    summary: ErrorSummary,
    errors: Vec<T>,
}

/// Displays the summary line followed by a list of the contained errors.
///
/// If the summary line contains the placeholder `{count}`, it will be replaced
/// with the number of errors (only the first occurrence will be replaced).
/// Otherwise, the summary line is printed as-is.
impl<T> Display for ErrorList<T>
where
    T: Display + Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.summary)?;

        f.write_char('\n')?;

        // Write the list of errors
        for error in &self.errors {
            f.write_str("- ")?;
            Display::fmt(&error, f)?;
            f.write_char('\n')?;
        }

        Ok(())
    }
}

impl<T> Debug for ErrorList<T>
where
    T: Display + Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ErrorList")
            .field("summary", &self.summary.as_cow().as_ref())
            .field("errors", &self.errors)
            .finish()?;
        Ok(())
    }
}

impl<T> Clone for ErrorList<T>
where
    T: Display + Debug + Clone,
{
    fn clone(&self) -> Self {
        Self {
            summary: self.summary.freeze(),
            errors: self.errors.clone(),
        }
    }
}

impl<T> Error for ErrorList<T> where T: Display + Debug {}

impl<T> Deref for ErrorList<T>
where
    T: Display + Debug,
{
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.errors
    }
}

impl<T> IntoIterator for ErrorList<T>
where
    T: Display + Debug,
{
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.errors.into_iter()
    }
}

impl<T> ErrorList<T>
where
    T: Display + Debug,
{
    /// Creates a new BoxedErrors with the given summary line and list of errors.
    ///
    /// The summary line can contain the placeholder `{count}`, which will be
    /// replaced with the number of errors when formatted.
    #[allow(dead_code)]
    pub(crate) fn new(summary: ErrorSummary, errors: Vec<T>) -> Self {
        let summary = summary.with_count(errors.len());
        Self { summary, errors }
    }

    pub fn to_vec(self) -> Vec<T> {
        self.errors
    }

    /// Returns the summary line as a String, with the `{count}` placeholder
    /// replaced with the number of errors.
    pub fn summary(&self) -> String {
        self.summary.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sets_summary_and_errors() {
        let errors = vec!["err1", "err2"];

        let list = ErrorList::new("Found {count} issues".into(), errors.clone());

        assert_eq!(list.summary(), "Found 2 issues");
        assert_eq!(&*list, errors.as_slice());
    }
}
