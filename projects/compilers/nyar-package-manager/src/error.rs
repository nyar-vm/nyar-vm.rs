use miette::Diagnostic;
use nyar_package_registry::RegistryError;
use std_data::text::von::VonError;
use thiserror::Error;

/// Package manager errors.
#[derive(Debug, Error, Diagnostic)]
pub enum PackageManagerError {
    #[error(transparent)]
    #[diagnostic(code(nyar::package_manager::registry))]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    #[diagnostic(code(nyar::package_manager::von))]
    Von(#[from] VonError),
    #[error(transparent)]
    #[diagnostic(code(nyar::package_manager::io))]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    #[diagnostic(code(nyar::package_manager::message))]
    Message(String),
}

impl PackageManagerError {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

pub type Result<T> = std::result::Result<T, PackageManagerError>;
