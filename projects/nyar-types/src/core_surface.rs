//! Versioned, closed core-surface registry.

/// A language construct admitted by a core-surface manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CoreFeature {
    PackageImports,
    Functions,
    ControlFlow,
    NominalTypes,
    Nullable,
    Option,
    Utf8,
    Utf16,
    Float64,
    TypedDeclarations,
}

/// Explicit versioned allow-list. Unknown constructs must be rejected by validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreSurfaceManifest {
    pub version: String,
    features: Vec<CoreFeature>,
}

impl CoreSurfaceManifest {
    /// Creates a manifest; an empty version or feature list is invalid.
    pub fn new(version: impl Into<String>, features: impl IntoIterator<Item = CoreFeature>) -> Result<Self, &'static str> {
        let manifest = Self { version: version.into(), features: features.into_iter().collect() };
        if manifest.version.trim().is_empty() || manifest.features.is_empty() {
            return Err("core surface manifest is incomplete");
        }
        Ok(manifest)
    }
    /// Returns whether a feature is explicitly admitted.
    pub fn admits(&self, feature: CoreFeature) -> bool {
        self.features.contains(&feature)
    }
    /// Rejects a construct outside the versioned core surface.
    pub fn require(&self, feature: CoreFeature) -> Result<(), &'static str> {
        self.admits(feature).then_some(()).ok_or("construct is outside the core surface")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_features_fail_closed() {
        let manifest = CoreSurfaceManifest::new("1", [CoreFeature::Functions]).unwrap();
        assert!(manifest.require(CoreFeature::Functions).is_ok());
        assert!(manifest.require(CoreFeature::Float64).is_err());
    }
}
