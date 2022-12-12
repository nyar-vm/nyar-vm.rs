use std::error::Error;
use std::fmt::Display;
use std::fmt::{Debug, Formatter};

mod convert;
mod display;

/// The result type of this crate.
pub type Result<T> = std::result::Result<T, GroovyError>;

/// A boxed error kind, wrapping an [GroovyErrorKind].
#[derive(Clone)]
pub struct GroovyError {
    kind: Box<GroovyErrorKind>,
}

/// The kind of [GroovyError].
#[derive(Debug, Copy, Clone)]
pub enum GroovyErrorKind {
    /// An unknown error.
    UnknownError,
}
