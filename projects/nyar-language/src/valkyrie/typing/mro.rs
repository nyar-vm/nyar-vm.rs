use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Errors produced while computing or validating linearized inheritance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MroError {
    /// The parent sequences cannot be merged into a valid C3 order.
    InconsistentHierarchy {
        /// The class currently being linearized.
        class: String,
        /// Human-readable details about the merge failure.
        details: String,
    },
    /// The inheritance graph contains a cycle.
    CircularInheritance {
        /// The cycle that was detected in declaration order.
        chain: Vec<String>,
    },
}

impl std::fmt::Display for MroError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InconsistentHierarchy { class, details } => {
                write!(f, "inconsistent hierarchy for {class}: {details}")
            }
            Self::CircularInheritance { chain } => {
                write!(f, "circular inheritance detected: {}", chain.join(" -> "))
            }
        }
    }
}

impl std::error::Error for MroError {}

/// A direct parent entry with an optional renamed-access alias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParentInfo {
    /// The nominal parent type name.
    pub name: String,
    /// The explicit slot alias used for qualified access.
    pub alias: Option<String>,
}

/// Whether an inherited member already has a concrete implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberImplementation {
    /// The member can be selected directly by linearization.
    Concrete,
    /// The member remains abstract and still requires an override.
    Abstract,
}

/// A member contributed by one inherited parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberSource {
    /// The parent that contributes this member.
    pub parent: String,
    /// Whether the contributed member is concrete or abstract.
    pub implementation: MemberImplementation,
}

impl MemberSource {
    /// Creates a concrete inherited member source.
    pub fn concrete(parent: impl Into<String>) -> Self {
        Self { parent: parent.into(), implementation: MemberImplementation::Concrete }
    }

    /// Creates an abstract inherited member source.
    pub fn abstract_member(parent: impl Into<String>) -> Self {
        Self { parent: parent.into(), implementation: MemberImplementation::Abstract }
    }
}

/// The resolution state for one inherited method name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodConflict {
    /// The inherited method name.
    pub method: String,
    /// Direct parents that contribute this method.
    pub direct_parents: Vec<String>,
    /// The concrete parent chosen by C3, if any.
    pub mro_winner: Option<String>,
    /// Whether the child already provides its own override.
    pub overridden_in_child: bool,
    /// Whether the child must provide an override to become concrete.
    pub requires_override: bool,
}

/// The resolution state for one inherited property name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyConflict {
    /// The inherited property name.
    pub property: String,
    /// Direct parents that contribute this property.
    pub direct_parents: Vec<String>,
    /// The concrete parent chosen by C3, if any.
    pub mro_winner: Option<String>,
    /// Whether the child already provides its own override.
    pub overridden_in_child: bool,
    /// Whether the child must provide an override to become concrete.
    pub requires_override: bool,
    /// Whether the merged contract requires a getter.
    pub requires_getter: bool,
    /// Whether the merged contract requires a setter.
    pub requires_setter: bool,
}

/// A property contributed by one inherited parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertySource {
    /// The parent that contributes this property.
    pub parent: String,
    /// Whether the contributed property is concrete or abstract.
    pub implementation: MemberImplementation,
    /// Whether the property contract includes a getter.
    pub requires_getter: bool,
    /// Whether the property contract includes a setter.
    pub requires_setter: bool,
}

impl PropertySource {
    /// Creates a concrete inherited property source.
    pub fn concrete(parent: impl Into<String>, requires_getter: bool, requires_setter: bool) -> Self {
        Self { parent: parent.into(), implementation: MemberImplementation::Concrete, requires_getter, requires_setter }
    }

    /// Creates an abstract inherited property source.
    pub fn abstract_property(parent: impl Into<String>, requires_getter: bool, requires_setter: bool) -> Self {
        Self { parent: parent.into(), implementation: MemberImplementation::Abstract, requires_getter, requires_setter }
    }
}

/// Stateless C3 linearization entry point.
pub struct C3Linearization;

impl C3Linearization {
    /// Computes the C3 method resolution order for one class.
    pub fn compute(class: &str, parent_mros: Vec<Vec<String>>) -> Result<Vec<String>, MroError> {
        if parent_mros.iter().flatten().any(|item| item == class) {
            return Err(MroError::CircularInheritance { chain: vec![class.to_string(), class.to_string()] });
        }

        let direct_parents = parent_mros
            .iter()
            .map(|mro| {
                mro.first().cloned().ok_or_else(|| MroError::InconsistentHierarchy {
                    class: class.to_string(),
                    details: "parent linearization cannot be empty".to_string(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut sequences = parent_mros.into_iter().map(VecDeque::from).collect::<Vec<_>>();
        sequences.push(VecDeque::from(direct_parents));

        let mut linearization = vec![class.to_string()];

        loop {
            sequences.retain(|sequence| !sequence.is_empty());
            if sequences.is_empty() {
                return Ok(linearization);
            }

            let Some(candidate) = select_c3_head(&sequences)
            else {
                return Err(MroError::InconsistentHierarchy {
                    class: class.to_string(),
                    details: format!("cannot merge sequences {:?}", render_sequences(&sequences)),
                });
            };

            linearization.push(candidate.clone());

            for sequence in &mut sequences {
                if sequence.front().is_some_and(|head| head == &candidate) {
                    sequence.pop_front();
                }
            }
        }
    }
}

/// Resolves renamed parent access against direct parents and computed MRO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodResolver {
    parents: Vec<ParentInfo>,
    mro: Vec<String>,
}

impl MethodResolver {
    /// Creates a new resolver from direct parents and the final MRO.
    pub fn new(parents: Vec<ParentInfo>, mro: Vec<String>) -> Self {
        Self { parents, mro }
    }

    /// Resolves an explicit parent alias like `self.primary.method()`.
    pub fn resolve_qualified(&self, alias: &str) -> Option<&str> {
        self.parents.iter().find(|parent| parent.alias.as_deref() == Some(alias)).map(|parent| parent.name.as_str())
    }

    /// Returns `true` when the provided name is a declared parent alias.
    pub fn is_valid_alias(&self, alias: &str) -> bool {
        self.resolve_qualified(alias).is_some()
    }

    /// Resolves either a declared alias or a parent class name visible in the MRO.
    pub fn get_effective_parent(&self, qualifier: &str) -> Option<&str> {
        self.resolve_qualified(qualifier).or_else(|| self.mro.iter().find(|item| item.as_str() == qualifier).map(String::as_str))
    }
}

/// Analyzes inherited method names for ambiguity and required overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodConflictAnalyzer {
    direct_parents: Vec<ParentInfo>,
    mro: Vec<String>,
}

impl MethodConflictAnalyzer {
    /// Creates a new analyzer from direct parents and the final MRO.
    pub fn new(direct_parents: Vec<ParentInfo>, mro: Vec<String>) -> Self {
        Self { direct_parents, mro }
    }

    /// Analyzes inherited methods and reports which names still need an override.
    pub fn analyze(&self, inherited_members: BTreeMap<String, Vec<MemberSource>>, child_methods: &[String]) -> Vec<MethodConflict> {
        let child_methods = child_methods.iter().cloned().collect::<BTreeSet<_>>();
        let mut conflicts = Vec::new();

        for (method, sources) in inherited_members {
            let direct_parents = self.direct_parent_sources(&sources);
            let overridden_in_child = child_methods.contains(&method);
            let has_concrete = sources.iter().any(|source| source.implementation == MemberImplementation::Concrete);
            let requires_override = !overridden_in_child && !has_concrete;

            if direct_parents.len() <= 1 && !requires_override {
                continue;
            }

            conflicts.push(MethodConflict {
                method,
                direct_parents,
                mro_winner: self.resolve_member_owner(&sources).map(str::to_string),
                overridden_in_child,
                requires_override,
            });
        }

        conflicts.sort_by(|lhs, rhs| lhs.method.cmp(&rhs.method));
        conflicts
    }

    /// Resolves which parent currently wins for one member under the MRO.
    pub fn resolve_member_owner<'a>(&'a self, sources: &'a [MemberSource]) -> Option<&'a str> {
        self.mro
            .iter()
            .find_map(|parent| sources.iter().find(|source| &source.parent == parent).map(|source| source.parent.as_str()))
            .or_else(|| sources.first().map(|source| source.parent.as_str()))
    }

    fn direct_parent_sources(&self, sources: &[MemberSource]) -> Vec<String> {
        let source_names = sources.iter().map(|source| source.parent.as_str()).collect::<BTreeSet<_>>();
        self.direct_parents.iter().filter(|parent| source_names.contains(parent.name.as_str())).map(|parent| parent.name.clone()).collect()
    }
}

/// Analyzes inherited property names for ambiguity and required overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyConflictAnalyzer {
    direct_parents: Vec<ParentInfo>,
    mro: Vec<String>,
}

impl PropertyConflictAnalyzer {
    /// Creates a new analyzer from direct parents and the final MRO.
    pub fn new(direct_parents: Vec<ParentInfo>, mro: Vec<String>) -> Self {
        Self { direct_parents, mro }
    }

    /// Analyzes inherited properties and reports which names still need an override.
    pub fn analyze(&self, inherited_properties: BTreeMap<String, Vec<PropertySource>>, child_properties: &[String]) -> Vec<PropertyConflict> {
        let child_properties = child_properties.iter().cloned().collect::<BTreeSet<_>>();
        let mut conflicts = Vec::new();

        for (property, sources) in inherited_properties {
            let direct_parents = self.direct_parent_sources(&sources);
            let overridden_in_child = child_properties.contains(&property);
            let has_concrete = sources.iter().any(|source| source.implementation == MemberImplementation::Concrete);
            let requires_override = !overridden_in_child && !has_concrete;

            if direct_parents.len() <= 1 && !requires_override {
                continue;
            }

            conflicts.push(PropertyConflict {
                property,
                direct_parents,
                mro_winner: self.resolve_property_owner(&sources).map(str::to_string),
                overridden_in_child,
                requires_override,
                requires_getter: sources.iter().any(|source| source.requires_getter),
                requires_setter: sources.iter().any(|source| source.requires_setter),
            });
        }

        conflicts.sort_by(|lhs, rhs| lhs.property.cmp(&rhs.property));
        conflicts
    }

    /// Resolves which parent currently wins for one property under the MRO.
    pub fn resolve_property_owner<'a>(&'a self, sources: &'a [PropertySource]) -> Option<&'a str> {
        self.mro
            .iter()
            .find_map(|parent| sources.iter().find(|source| &source.parent == parent).map(|source| source.parent.as_str()))
            .or_else(|| sources.first().map(|source| source.parent.as_str()))
    }

    fn direct_parent_sources(&self, sources: &[PropertySource]) -> Vec<String> {
        let source_names = sources.iter().map(|source| source.parent.as_str()).collect::<BTreeSet<_>>();
        self.direct_parents.iter().filter(|parent| source_names.contains(parent.name.as_str())).map(|parent| parent.name.clone()).collect()
    }
}

fn select_c3_head(sequences: &[VecDeque<String>]) -> Option<String> {
    'outer: for sequence in sequences {
        let candidate = sequence.front()?;
        for other in sequences {
            if other.iter().skip(1).any(|item| item == candidate) {
                continue 'outer;
            }
        }
        return Some(candidate.clone());
    }
    None
}

fn render_sequences(sequences: &[VecDeque<String>]) -> Vec<Vec<String>> {
    sequences.iter().map(|sequence| sequence.iter().cloned().collect()).collect()
}
