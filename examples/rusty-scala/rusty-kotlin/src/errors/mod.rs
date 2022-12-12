use std::error::Error;
use std::fmt::Display;
use std::fmt::{Debug, Formatter};

mod convert;
mod display;

/// The result type of this crate.
pub type Result<T> = std::result::Result<T, KotlinError>;

/// A boxed error kind, wrapping an [KotlinErrorKind].
#[derive(Clone)]
pub struct KotlinError {
    kind: Box<KotlinErrorKind>,
}

/// The kind of [KotlinError].
#[derive(Debug, Copy, Clone)]
pub enum KotlinErrorKind {
    /// An unknown error.
    UnknownError,
}
