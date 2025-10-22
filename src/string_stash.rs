use std::borrow::Cow;
use core::error::Error;
use core::fmt::{Debug, Display};
use core::ops::Deref;

pub type Location = &'static core::panic::Location<'static>;

/// A simple error type that holds a string message, and optionally
/// a location where the error was created.
struct StringError {
    message: Cow<'static, str>,
    location: Option<Location>,
}

impl Deref for StringError {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.message
    }
}

impl Debug for StringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StringError")
            .field("message", &self.message)
            .field("location", &self.location)
            .finish()
    }
}

impl Display for StringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let is_pretty = f.alternate();
        match (is_pretty, &self.location) {
            (true, Some(loc)) => {
                writeln!(f, "{}", self.message)?;
                write!(f, "  at {:?}", loc)
            }
            _ => write!(f, "{}", self.message),
        }
    }
}

impl Error for StringError {}