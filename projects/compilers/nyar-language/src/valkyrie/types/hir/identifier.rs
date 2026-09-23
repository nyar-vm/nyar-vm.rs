//! Identifier with shadowing information for HIR.

use crate::{Identifier, SourceSpan};

/// An identifier with shadowing information.
///
/// This structure represents an identifier in the HIR that tracks
/// shadowing through a shadow index. When variables with the same
/// name are declared in nested scopes, each declaration gets a
/// unique shadow index to distinguish them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirIdentifier {
    /// The identifier name.
    pub name: Identifier,
    /// The shadow index for distinguishing shadowed variables.
    ///
    /// A value of 0 indicates the first occurrence of the name.
    /// Higher values indicate shadowed occurrences in nested scopes.
    pub shadow_index: u32,
    /// The source span for error reporting.
    pub span: SourceSpan,
}
