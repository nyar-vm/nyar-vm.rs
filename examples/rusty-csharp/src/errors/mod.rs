use std::error::Error;
use std::fmt::Display;
use std::fmt::{Debug, Formatter};

mod convert;
mod display;

/// The result type of this crate.
pub type Result<T> = std::result::Result<T, ExampleError>;

/// A boxed error kind, wrapping an [ExampleErrorKind].
#[derive(Clone)]
pub struct ExampleError {
    kind: Box<ExampleErrorKind>,
}

/// The kind of [ExampleError].
#[derive(Debug, Copy, Clone)]
pub enum ExampleErrorKind {
    /// An unknown error.
    UnknownError,
}
