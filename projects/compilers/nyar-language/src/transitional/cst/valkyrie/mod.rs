#![doc = include_str!("readme.md")]
#![warn(missing_docs)]

/// Parser-facing AST node family.
pub mod ast;
/// Concrete syntax tree (lossless, for formatter).
pub mod cst;
/// Lexical analysis entry points and token definitions.
pub mod lexer;
/// Semantic naming validation (`snake_case` enforcement).
pub mod naming;
/// Layered parser tree text snapshots.
pub mod parse_dump;
/// Source-to-AST parsing entry points.
pub mod parser;
/// T-Grammar / `<% %>` meta-level templates (Valkyrie language extension).
pub mod tgrammar;
/// X-Grammar / XML inline markup (Valkyrie language extension).
pub mod xml;

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
