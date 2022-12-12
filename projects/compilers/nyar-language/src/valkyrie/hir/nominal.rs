//! Minimal nominal typing and unite-lowering helpers.
//!
//! These helpers intentionally model only the currently settled semantics:
//! classes use nominal exact-or-subtype matching, and `unite` lowers to
//! an abstract sealed base with a closed family of nominal variants.

#![allow(missing_docs)]

use std::collections::{BTreeMap, BTreeSet};

use crate::types::{
    Identifier, NamePath,
    hir::{
        GenericType, HirDocumentation, HirEnum, HirKind, HirModule, HirParent, HirStruct, HirVariant, HirVisibility, TraitObject, TypeLambda,
        ValkyrieType,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UniteLayout {
    Tagged,
    Untagged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredUnite {
    pub base: HirStruct,
    pub variants: Vec<HirStruct>,
    pub layout: UniteLayout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UniteCoverageError {
    MissingVariants { names: Vec<Identifier> },
    UnknownVariants { names: Vec<Identifier> },
    DuplicateVariants { names: Vec<Identifier> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UniteDefinitionError {
    EmptyVariants,
    DuplicateVariants { names: Vec<Identifier> },
    InvalidVariantResultType { variant: Identifier },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NominalModuleError {
    UnknownType { name: Identifier },
    InvalidUnite { name: Identifier, error: UniteDefinitionError },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NominalModuleView {
    named_types: BTreeMap<Identifier, HirStruct>,
    unite_defs: BTreeMap<Identifier, HirEnum>,
    invalid_unites: BTreeMap<Identifier, UniteDefinitionError>,
}

impl LoweredUnite {
    pub fn variant_names(&self) -> Vec<Identifier> {
        self.variants.iter().map(|variant| variant.name.clone()).collect()
    }

    pub fn is_exhaustive_over(&self, covered_variants: &[Identifier]) -> bool {
        let covered: BTreeSet<_> = covered_variants.iter().cloned().collect();
        self.variant_names().into_iter().all(|name| covered.contains(&name))
    }

    pub fn check_exhaustiveness(&self, covered_variants: &[Identifier]) -> Result<(), UniteCoverageError> {
        let declared = self.variant_names().into_iter().collect::<BTreeSet<_>>();
        let duplicates = duplicate_names(covered_variants.iter().cloned()).into_iter().collect::<Vec<_>>();

        if !duplicates.is_empty() {
            return Err(UniteCoverageError::DuplicateVariants { names: duplicates });
        }

        let covered = covered_variants.iter().cloned().collect::<BTreeSet<_>>();
        let unknown = covered.difference(&declared).cloned().collect::<Vec<_>>();
        if !unknown.is_empty() {
            return Err(UniteCoverageError::UnknownVariants { names: unknown });
        }

        let missing = declared.difference(&covered).cloned().collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(UniteCoverageError::MissingVariants { names: missing });
        }

        Ok(())
    }
}

impl NominalModuleView {
    pub fn from_module(module: &HirModule) -> Self {
        let mut named_types = module.structs.iter().map(|item| (item.name.clone(), item.clone())).collect::<BTreeMap<_, _>>();
        let mut unite_defs = BTreeMap::new();
        let mut invalid_unites = BTreeMap::new();

        // Imported nominal definitions are semantic exports from dependencies,
        // not source text to be reparsed.  They participate in exactly the
        // same nominal relation and layout checks as local definitions.
        for enum_def in module.enums.iter().chain(module.imported_nominal_enums()) {
            if !enum_def.is_unity() {
                continue;
            }

            unite_defs.insert(enum_def.name.clone(), enum_def.clone());

            match validate_unite_definition(enum_def) {
                Ok(()) => {
                    let lowered = lower_unite(enum_def, UniteLayout::Untagged);
                    named_types.insert(lowered.base.name.clone(), lowered.base);
                    for variant in lowered.variants {
                        named_types.insert(variant.name.clone(), variant);
                    }
                }
                Err(error) => {
                    invalid_unites.insert(enum_def.name.clone(), error);
                }
            }
        }

        Self { named_types, unite_defs, invalid_unites }
    }

    pub fn validate_unite(&self, name: &Identifier) -> Result<(), NominalModuleError> {
        if let Some(error) = self.invalid_unites.get(name) {
            return Err(NominalModuleError::InvalidUnite { name: name.clone(), error: error.clone() });
        }

        if self.unite_defs.contains_key(name) {
            return Ok(());
        }

        Err(NominalModuleError::UnknownType { name: name.clone() })
    }

    pub fn lower_unite(&self, name: &Identifier, layout: UniteLayout) -> Result<LoweredUnite, NominalModuleError> {
        self.validate_unite(name)?;
        let enum_def = self.unite_defs.get(name).ok_or_else(|| NominalModuleError::UnknownType { name: name.clone() })?;
        Ok(lower_unite(enum_def, layout))
    }

    pub fn matches_nominal_parameter(&self, candidate: &Identifier, expected: &Identifier) -> Result<bool, NominalModuleError> {
        let candidate_struct = self.lookup_named_type(candidate)?;
        let expected_struct = self.lookup_named_type(expected)?;
        let declared_types = self.named_types.values().cloned().collect::<Vec<_>>();
        Ok(matches_nominal_parameter(candidate_struct, expected_struct, &declared_types))
    }

    pub fn nominal_match_distance(&self, candidate: &Identifier, expected: &Identifier) -> Result<Option<usize>, NominalModuleError> {
        let candidate_struct = self.lookup_named_type(candidate)?;
        let expected_struct = self.lookup_named_type(expected)?;
        let declared_types = self.named_types.values().cloned().collect::<Vec<_>>();
        Ok(nominal_parameter_distance(candidate_struct, expected_struct, &declared_types))
    }

    fn lookup_named_type(&self, name: &Identifier) -> Result<&HirStruct, NominalModuleError> {
        if let Some(found) = self.named_types.get(name) {
            return Ok(found);
        }

        if let Some(error) = self.invalid_unites.get(name) {
            return Err(NominalModuleError::InvalidUnite { name: name.clone(), error: error.clone() });
        }

        Err(NominalModuleError::UnknownType { name: name.clone() })
    }
}

pub fn matches_nominal_parameter(candidate: &HirStruct, expected: &HirStruct, declared_types: &[HirStruct]) -> bool {
    if candidate.name == expected.name {
        return true;
    }

    let index = declared_types.iter().map(|item| (item.name.clone(), item)).collect::<BTreeMap<_, _>>();

    inherits_from(candidate, &expected.name, &index, &mut BTreeSet::new())
}

pub fn nominal_parameter_distance(candidate: &HirStruct, expected: &HirStruct, declared_types: &[HirStruct]) -> Option<usize> {
    if candidate.name == expected.name {
        return Some(0);
    }

    let index = declared_types.iter().map(|item| (item.name.clone(), item)).collect::<BTreeMap<_, _>>();
    inheritance_distance(candidate, &expected.name, &index, &mut BTreeSet::new())
}

pub fn validate_unite_definition(enum_def: &HirEnum) -> Result<(), UniteDefinitionError> {
    if enum_def.variants.is_empty() {
        return Err(UniteDefinitionError::EmptyVariants);
    }

    let duplicates = duplicate_names(enum_def.variants.iter().map(|variant| variant.name.clone())).into_iter().collect::<Vec<_>>();
    if !duplicates.is_empty() {
        return Err(UniteDefinitionError::DuplicateVariants { names: duplicates });
    }

    for variant in &enum_def.variants {
        if let Some(result_type) = variant.result_type.as_ref() {
            if parse_variant_result_type(enum_def, result_type).is_none() || !references_only_declared_generics(&enum_def.generics, result_type)
            {
                return Err(UniteDefinitionError::InvalidVariantResultType { variant: variant.name.clone() });
            }
        }
    }

    Ok(())
}

pub fn lower_unite(enum_def: &HirEnum, layout: UniteLayout) -> LoweredUnite {
    validate_unite_definition(enum_def).expect("invalid unite definition");
    let base_name = enum_def.name.clone();
    let base_path = NamePath::new(vec![base_name.clone()]);
    let parent_generics = enum_def.generics.iter().map(lower_unite_generic_argument).collect::<Vec<_>>();

    let base = HirStruct {
        name: base_name,
        namespace: vec![],
        doc: enum_def.doc.clone(),
        generics: enum_def.generics.clone(),
        parents: vec![],
        fields: vec![],
        methods: vec![],
        properties: vec![],
        visibility: enum_def.visibility,
        is_value_type: false,
        is_abstract: true,
        is_sealed: true,
        is_final: false,
        is_open: false,
        abstract_methods: vec![],
        abstract_properties: vec![],
        derives: vec![],
    };

    let variants =
        enum_def.variants.iter().map(|variant| lower_variant(variant, enum_def, &base_path, &parent_generics, enum_def.visibility)).collect();

    LoweredUnite { base, variants, layout }
}

fn inherits_from(
    candidate: &HirStruct,
    expected: &Identifier,
    index: &BTreeMap<Identifier, &HirStruct>,
    visiting: &mut BTreeSet<Identifier>,
) -> bool {
    if !visiting.insert(candidate.name.clone()) {
        return false;
    }

    let found = candidate.parents.iter().any(|parent| {
        let Some(parent_name) = parent_name(parent)
        else {
            return false;
        };

        if &parent_name == expected {
            return true;
        }

        index.get(&parent_name).is_some_and(|parent_struct| inherits_from(parent_struct, expected, index, visiting))
    });

    visiting.remove(&candidate.name);
    found
}

fn inheritance_distance(
    candidate: &HirStruct,
    expected: &Identifier,
    index: &BTreeMap<Identifier, &HirStruct>,
    visiting: &mut BTreeSet<Identifier>,
) -> Option<usize> {
    if !visiting.insert(candidate.name.clone()) {
        return None;
    }

    let best = candidate
        .parents
        .iter()
        .filter_map(|parent| {
            let parent_name = parent_name(parent)?;

            if &parent_name == expected {
                return Some(1);
            }

            let parent_struct = index.get(&parent_name)?;
            inheritance_distance(parent_struct, expected, index, visiting).map(|distance| distance + 1)
        })
        .min();

    visiting.remove(&candidate.name);
    best
}

fn parent_name(parent: &HirParent) -> Option<Identifier> {
    parent.name.parts().last().cloned()
}

fn lower_variant(
    variant: &HirVariant,
    enum_def: &HirEnum,
    base_path: &NamePath,
    base_parent_generics: &[ValkyrieType],
    visibility: HirVisibility,
) -> HirStruct {
    let parent_generics = variant
        .result_type
        .as_ref()
        .and_then(|result_type| parse_variant_result_type(enum_def, result_type))
        .unwrap_or_else(|| base_parent_generics.to_vec());
    let generics = variant
        .result_type
        .as_ref()
        .map(|result_type| referenced_generics(&enum_def.generics, result_type))
        .unwrap_or_else(|| enum_def.generics.clone());

    HirStruct {
        name: variant.name.clone(),
        namespace: vec![],
        doc: HirDocumentation::default(),
        generics,
        parents: vec![HirParent::with_generics(base_path.clone(), parent_generics)],
        fields: variant.fields.clone(),
        methods: vec![],
        properties: vec![],
        visibility,
        is_value_type: false,
        is_abstract: false,
        is_sealed: true,
        is_final: true,
        is_open: false,
        abstract_methods: vec![],
        abstract_properties: vec![],
        derives: vec![],
    }
}

fn lower_unite_generic_argument(generic: &GenericType) -> ValkyrieType {
    ValkyrieType::Generic(GenericType { name: generic.name.clone(), kind: lower_unite_kind(&generic.kind), bounds: generic.bounds.clone() })
}

fn lower_unite_kind(kind: &HirKind) -> HirKind {
    match kind {
        HirKind::Type => HirKind::Type,
        HirKind::Function(input, output) => HirKind::Function(Box::new(lower_unite_kind(input)), Box::new(lower_unite_kind(output))),
    }
}

fn parse_variant_result_type(enum_def: &HirEnum, result_type: &ValkyrieType) -> Option<Vec<ValkyrieType>> {
    match result_type {
        ValkyrieType::Named(name) if name == &enum_def.name && enum_def.generics.is_empty() => Some(vec![]),
        ValkyrieType::Apply(base, args) => match base.as_ref() {
            ValkyrieType::Named(name) if name == &enum_def.name && args.len() == enum_def.generics.len() => Some(args.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn referenced_generics(declared: &[GenericType], ty: &ValkyrieType) -> Vec<GenericType> {
    let mut names = BTreeSet::new();
    collect_generic_names(ty, &mut names);
    declared.iter().filter(|generic| names.contains(&generic.name)).cloned().collect()
}

fn references_only_declared_generics(declared: &[GenericType], ty: &ValkyrieType) -> bool {
    let declared_names = declared.iter().map(|generic| generic.name.clone()).collect::<BTreeSet<_>>();
    let mut referenced_names = BTreeSet::new();
    collect_generic_names(ty, &mut referenced_names);
    referenced_names.is_subset(&declared_names)
}

fn collect_generic_names(ty: &ValkyrieType, names: &mut BTreeSet<Identifier>) {
    match ty {
        ValkyrieType::Generic(GenericType { name, .. }) => {
            names.insert(name.clone());
        }
        ValkyrieType::Apply(base, args) => {
            collect_generic_names(base, names);
            for arg in args {
                collect_generic_names(arg, names);
            }
        }
        ValkyrieType::Function(function) => {
            for param in &function.params {
                collect_generic_names(param, names);
            }
            collect_generic_names(&function.return_type, names);
        }
        ValkyrieType::Tuple(items) => {
            for item in items {
                collect_generic_names(item, names);
            }
        }
        ValkyrieType::Row(row) => {
            for method in &row.methods {
                for param in &method.params {
                    collect_generic_names(param, names);
                }
                collect_generic_names(&method.return_type, names);
            }
        }
        ValkyrieType::Array(item) => {
            collect_generic_names(item, names);
        }
        ValkyrieType::FixedArray { element, .. } => {
            collect_generic_names(element, names);
        }
        ValkyrieType::TypeLambda(type_lambda) => {
            let TypeLambda { params: _, body } = type_lambda.as_ref();
            collect_generic_names(body, names);
        }
        ValkyrieType::TraitObject(TraitObject { type_arguments: type_args, .. }) => {
            for arg in type_args {
                collect_generic_names(arg, names);
            }
        }
        ValkyrieType::Associated(associated) => {
            collect_generic_names(&associated.base, names);
            for arg in &associated.type_arguments {
                collect_generic_names(arg, names);
            }
        }
        ValkyrieType::Integer8 { .. }
        | ValkyrieType::Integer16 { .. }
        | ValkyrieType::Integer32 { .. }
        | ValkyrieType::Integer64 { .. }
        | ValkyrieType::Integer128 { .. }
        | ValkyrieType::Float32
        | ValkyrieType::Float64
        | ValkyrieType::Boolean
        | ValkyrieType::Character
        | ValkyrieType::Utf8
        | ValkyrieType::Utf16
        | ValkyrieType::Unit
        | ValkyrieType::Void
        | ValkyrieType::AutoType
        | ValkyrieType::Named(_)
        | ValkyrieType::r#SelfType
        | ValkyrieType::Nullable(_)
        | ValkyrieType::Union(_)
        | ValkyrieType::Intersection(_) => {}
    }
}

fn duplicate_names(names: impl Iterator<Item = Identifier>) -> BTreeSet<Identifier> {
    let mut counts = BTreeMap::new();

    for name in names {
        *counts.entry(name).or_insert(0usize) += 1;
    }

    counts.into_iter().filter_map(|(name, count)| (count > 1).then_some(name)).collect()
}
