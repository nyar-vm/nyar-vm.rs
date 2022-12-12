use std::error::Error;
use std::fmt::Display;
use std::fmt::{Debug, Formatter};

mod convert;
mod display;

/// The result type of this crate.
pub type Result<T> = std::result::Result<T, ClojureError>;

/// A boxed error kind, wrapping an [ClojureErrorKind].
#[derive(Clone)]
pub struct ClojureError {
    kind: Box<ClojureErrorKind>,
}

/// The kind of [ClojureError].
#[derive(Debug, Copy, Clone)]
pub enum ClojureErrorKind {
    /// An unknown error.
    UnknownError,
}
