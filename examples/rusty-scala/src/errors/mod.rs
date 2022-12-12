use std::error::Error;
use std::fmt::Display;
use std::fmt::{Debug, Formatter};

mod convert;
mod display;

/// The result type of this crate.
pub type Result<T> = std::result::Result<T, ScalaError>;

/// A boxed error kind, wrapping an [ScalaErrorKind].
#[derive(Clone)]
pub struct ScalaError {
    kind: Box<ScalaErrorKind>,
}

/// The kind of [ScalaError].
#[derive(Debug, Copy, Clone)]
pub enum ScalaErrorKind {
    /// An unknown error.
    UnknownError,
}
