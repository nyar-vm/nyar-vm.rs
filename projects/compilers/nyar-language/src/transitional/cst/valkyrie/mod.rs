#![doc = include_str!("readme.md")]
#![warn(missing_docs)]

//! Legacy Valkyrie CST / parser（仅 `#[cfg(test)]` 对照；生产解析在 `oak-valkyrie`）。

/// Parser-facing AST node family.
#[cfg(test)]
pub mod ast;
/// Concrete syntax tree (lossless, for formatter).
#[cfg(test)]
pub mod cst;
/// Lexical analysis entry points and token definitions.
#[cfg(test)]
pub mod lexer;
/// Semantic naming validation (`snake_case` enforcement).
#[cfg(test)]
pub mod naming;
/// Layered parser tree text snapshots.
#[cfg(test)]
pub mod parse_dump;
/// Source-to-AST parsing entry points.
#[cfg(test)]
pub mod parser;
/// T-Grammar / `<% %>` meta-level templates (Valkyrie language extension).
#[cfg(test)]
pub mod tgrammar;
/// X-Grammar / XML inline markup (Valkyrie language extension).
#[cfg(test)]
pub mod xml;

#[cfg(test)]
pub use self::{
    ast::{
        Annotations, AttributeArgument, AttributeDeclaration, AttributeItem, AttributeList, BinaryOperator, ClassDeclaration, ClassLikeKind,
        DeclarationBody, FlagsDeclaration, FlagsMemberDeclaration, FunctionDeclKind, FunctionDeclaration, FunctionParameter, FunctionStatement,
        GenericParameterDeclaration, ImplyAssociatedConstBinding, ImplyAssociatedTypeBinding, ImplyDeclaration, InheritanceItem, LetStatement,
        LiteralExpression, MacroAssignDeclaration, NamePath, NamespaceDeclaration, ObjectBody, ObjectFieldDeclaration, ObjectMethodDeclaration,
        ParameterBindingKind, ParameterVariadicKind, PatternExpression, RootStatement, RowMethodTypeExpression, StringLiteral, StringSegment,
        SubscriptKind, SumTypeKind, TermCallArgument, TermExpression, TestsDeclaration, TraitAssociatedConstDeclaration,
        TraitAssociatedTypeDeclaration, TraitDeclaration, TypeExpression, TypePath, UnaryOperator, UniteDeclaration, UniteVariantDeclaration,
        UsingStatement, ValkyrieRoot, WhereConstraintDeclaration,
    },
    cst::{ValCstElement, ValCstParser, ValCstRoot, ValSyntaxKind},
    naming::{DIAG_ABI_BINDING_NOT_SNAKE_CASE, DIAG_IDENTIFIER_NOT_SNAKE_CASE, NamingViolation, naming_message, validate_snake_case},
    parser::{AstParser, ParseError, fixup_vx_widget_view_markup},
};
