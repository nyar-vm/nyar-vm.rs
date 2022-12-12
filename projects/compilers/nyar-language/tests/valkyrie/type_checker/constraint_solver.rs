use nyar_language::type_checker::*;

use nyar_language::types::{
    hir::{AssociatedType, FunctionType, GenericType, ValkyrieType},
    Identifier, NamePath,
};

fn make_trait_name(name: &str) -> NamePath {
    NamePath::new(vec![Identifier::new(name)])
}

#[test]
fn test_type_var_display() {
    let var = TypeVar(42);
    assert_eq!(format!("{}", var), "?T42");
}

#[test]
fn test_lifetime_display() {
    let lifetime = Lifetime::new("a");
    assert_eq!(format!("{}", lifetime), "'a");

    let static_lifetime = Lifetime::static_lifetime();
    assert_eq!(format!("{}", static_lifetime), "'static");
}

#[test]
fn test_constraint_solver_new() {
    let solver = ConstraintSolver::new();
    assert!(solver.constraints().is_empty());
    assert!(solver.substitutions().is_empty());
}

#[test]
fn test_add_trait_bound_constraint() {
    let mut solver = ConstraintSolver::new();
    let var = TypeVar(0);
    let trait_name = make_trait_name("Display");

    solver.add_constraint(TypeConstraint::trait_bound(var.clone(), trait_name.clone(), None));

    assert_eq!(solver.constraints().len(), 1);
    assert!(solver.trait_bounds().contains_key(&var));
    assert!(solver.trait_bounds()[&var].contains(&trait_name));
}

#[test]
fn test_add_equality_constraint() {
    let mut solver = ConstraintSolver::new();
    let t1 = ValkyrieType::Integer64 { signed: true };
    let t2 = ValkyrieType::Integer64 { signed: true };

    solver.add_constraint(TypeConstraint::equality(t1, t2, None));

    assert_eq!(solver.constraints().len(), 1);
}

#[test]
fn test_solve_equality_same_types() {
    let mut solver = ConstraintSolver::new();
    solver.add_constraint(TypeConstraint::equality(ValkyrieType::Integer64 { signed: true }, ValkyrieType::Integer64 { signed: true }, None));

    let result = solver.solve();
    assert!(result.is_ok());
}

#[test]
fn test_solve_equality_different_types() {
    let mut solver = ConstraintSolver::new();
    solver.add_constraint(TypeConstraint::equality(ValkyrieType::Integer64 { signed: true }, ValkyrieType::Boolean, None));

    let result = solver.solve();
    assert!(result.is_err());
    if let Err(err) = result {
        assert!(matches!(err.kind, ConstraintErrorKind::TypeMismatch { .. }));
    }
}

#[test]
fn test_unify_primitives() {
    let mut solver = ConstraintSolver::new();

    assert!(solver.unify(&ValkyrieType::Integer32 { signed: true }, &ValkyrieType::Integer32 { signed: true }, None).is_ok());
    assert!(solver.unify(&ValkyrieType::Integer64 { signed: true }, &ValkyrieType::Integer64 { signed: true }, None).is_ok());
    assert!(solver.unify(&ValkyrieType::Float32, &ValkyrieType::Float32, None).is_ok());
    assert!(solver.unify(&ValkyrieType::Float64, &ValkyrieType::Float64, None).is_ok());
    assert!(solver.unify(&ValkyrieType::Boolean, &ValkyrieType::Boolean, None).is_ok());
    assert!(solver.unify(&ValkyrieType::Utf8, &ValkyrieType::Utf8, None).is_ok());
    assert!(solver.unify(&ValkyrieType::Unit, &ValkyrieType::Unit, None).is_ok());
}

#[test]
fn test_unify_infer() {
    let mut solver = ConstraintSolver::new();

    assert!(solver.unify(&ValkyrieType::AutoType, &ValkyrieType::Integer64 { signed: true }, None).is_ok());
    assert!(solver.unify(&ValkyrieType::Integer64 { signed: true }, &ValkyrieType::AutoType, None).is_ok());
    assert!(solver.unify(&ValkyrieType::AutoType, &ValkyrieType::AutoType, None).is_ok());
}

#[test]
fn test_unify_named_types() {
    let mut solver = ConstraintSolver::new();
    let name1 = Identifier::new("MyType");
    let name2 = Identifier::new("MyType");
    let name3 = Identifier::new("OtherType");

    assert!(solver.unify(&ValkyrieType::Named(name1.clone()), &ValkyrieType::Named(name2), None).is_ok());
    assert!(solver.unify(&ValkyrieType::Named(name1), &ValkyrieType::Named(name3), None).is_err());
}

#[test]
fn test_unify_array_types() {
    let mut solver = ConstraintSolver::new();

    let t1 = ValkyrieType::Array(Box::new(ValkyrieType::Integer64 { signed: true }));
    let t2 = ValkyrieType::Array(Box::new(ValkyrieType::Integer64 { signed: true }));
    assert!(solver.unify(&t1, &t2, None).is_ok());

    let t3 = ValkyrieType::Array(Box::new(ValkyrieType::Boolean));
    assert!(solver.unify(&t1, &t3, None).is_err());
}

#[test]
fn test_unify_tuple_types() {
    let mut solver = ConstraintSolver::new();

    let t1 = ValkyrieType::Tuple(vec![ValkyrieType::Integer64 { signed: true }, ValkyrieType::Boolean]);
    let t2 = ValkyrieType::Tuple(vec![ValkyrieType::Integer64 { signed: true }, ValkyrieType::Boolean]);
    assert!(solver.unify(&t1, &t2, None).is_ok());

    let t3 = ValkyrieType::Tuple(vec![ValkyrieType::Integer64 { signed: true }]);
    assert!(solver.unify(&t1, &t3, None).is_err());

    let t4 = ValkyrieType::Tuple(vec![ValkyrieType::Boolean, ValkyrieType::Integer64 { signed: true }]);
    assert!(solver.unify(&t1, &t4, None).is_err());
}

#[test]
fn test_unify_function_types() {
    let mut solver = ConstraintSolver::new();

    let t1 = ValkyrieType::Function(FunctionType {
        params: vec![ValkyrieType::Integer64 { signed: true }, ValkyrieType::Boolean],
        return_type: Box::new(ValkyrieType::Utf8),
    });
    let t2 = ValkyrieType::Function(FunctionType {
        params: vec![ValkyrieType::Integer64 { signed: true }, ValkyrieType::Boolean],
        return_type: Box::new(ValkyrieType::Utf8),
    });
    assert!(solver.unify(&t1, &t2, None).is_ok());

    let t3 = ValkyrieType::Function(FunctionType { params: vec![ValkyrieType::Integer64 { signed: true }], return_type: Box::new(ValkyrieType::Utf8) });
    assert!(solver.unify(&t1, &t3, None).is_err());
}

#[test]
fn test_check_builtin_trait() {
    let solver = ConstraintSolver::new();

    assert!(solver.check_trait_bound(&ValkyrieType::Integer64 { signed: true }, &make_trait_name("Copy")).unwrap());
    assert!(solver.check_trait_bound(&ValkyrieType::Integer64 { signed: true }, &make_trait_name("Clone")).unwrap());
    assert!(solver.check_trait_bound(&ValkyrieType::Integer64 { signed: true }, &make_trait_name("Eq")).unwrap());
    assert!(solver.check_trait_bound(&ValkyrieType::Integer64 { signed: true }, &make_trait_name("Send")).unwrap());
    assert!(solver.check_trait_bound(&ValkyrieType::Integer64 { signed: true }, &make_trait_name("Sync")).unwrap());
    assert!(solver.check_trait_bound(&ValkyrieType::Integer64 { signed: true }, &make_trait_name("Default")).unwrap());
}

#[test]
fn test_register_trait_impl() {
    let mut solver = ConstraintSolver::new();

    solver.register_trait_impl(TraitImpl {
        ty: ValkyrieType::Named(Identifier::new("MyType")),
        trait_name: make_trait_name("Display"),
        type_args: vec![],
    });

    let result = solver.check_trait_bound(&ValkyrieType::Named(Identifier::new("MyType")), &make_trait_name("Display"));
    assert!(result.unwrap());
}

#[test]
fn test_trait_not_implemented_error() {
    let solver = ConstraintSolver::new();

    let result = solver.check_trait_bound(&ValkyrieType::Named(Identifier::new("CustomType")), &make_trait_name("Display"));
    assert!(!result.unwrap());
}

#[test]
fn test_constraint_error_display() {
    let err = ConstraintError::trait_not_implemented("MyType", "Display", None);
    assert!(err.to_string().contains("MyType"));
    assert!(err.to_string().contains("Display"));

    let err = ConstraintError::type_mismatch(ValkyrieType::Integer64 { signed: true }, ValkyrieType::Boolean, None);
    assert!(err.to_string().contains("类型不匹配"));

    let err = ConstraintError::infinite_type(TypeVar(0), ValkyrieType::Integer64 { signed: true }, None);
    assert!(err.to_string().contains("无限类型"));

    let err = ConstraintError::ambiguous_type(TypeVar(0), None);
    assert!(err.to_string().contains("类型歧义"));
}

#[test]
fn test_is_subtype() {
    let solver = ConstraintSolver::new();

    assert!(solver.is_subtype(&ValkyrieType::Integer32 { signed: true }, &ValkyrieType::Integer64 { signed: true }));
    assert!(solver.is_subtype(&ValkyrieType::Float32, &ValkyrieType::Float64));
    assert!(solver.is_subtype(&ValkyrieType::Integer64 { signed: true }, &ValkyrieType::Integer64 { signed: true }));
    assert!(!solver.is_subtype(&ValkyrieType::Integer64 { signed: true }, &ValkyrieType::Integer32 { signed: true }));
    assert!(!solver.is_subtype(&ValkyrieType::Integer64 { signed: true }, &ValkyrieType::Boolean));
}

#[test]
fn test_solver_clear() {
    let mut solver = ConstraintSolver::new();
    solver.add_constraint(TypeConstraint::trait_bound(TypeVar(0), make_trait_name("Display"), None));
    solver.register_trait_impl(TraitImpl { ty: ValkyrieType::Integer64 { signed: true }, trait_name: make_trait_name("Clone"), type_args: vec![] });

    solver.clear();

    assert!(solver.constraints().is_empty());
    assert!(solver.substitutions().is_empty());
    assert!(solver.trait_bounds().is_empty());
}

#[test]
fn test_subtype_constraint() {
    let mut solver = ConstraintSolver::new();

    solver.add_constraint(TypeConstraint::subtype(ValkyrieType::Integer32 { signed: true }, ValkyrieType::Integer64 { signed: true }, None));

    let result = solver.solve();
    assert!(result.is_ok());
}

#[test]
fn test_subtype_constraint_failure() {
    let mut solver = ConstraintSolver::new();

    solver.add_constraint(TypeConstraint::subtype(ValkyrieType::Integer64 { signed: true }, ValkyrieType::Integer32 { signed: true }, None));

    let result = solver.solve();
    assert!(result.is_err());
}

#[test]
fn test_multi_trait_bound_check_success() {
    let mut solver = ConstraintSolver::new();
    let var = TypeVar(0);

    solver.add_multi_trait_bound(MultiTraitBound::new(var.clone(), vec![make_trait_name("Copy"), make_trait_name("Clone")], None));

    let result = solver.solve();
    assert!(result.is_ok());
}

#[test]
fn test_where_clause() {
    let mut solver = ConstraintSolver::new();

    let where_clause = WhereClause::new(
        vec![
            WhereBound {
                ty: ValkyrieType::Generic(GenericType { name: Identifier::new("T"), kind: nyar_language::types::hir::HirKind::Type, bounds: vec![] }),
                traits: vec![make_trait_name("Display")],
                span: None,
            },
            WhereBound {
                ty: ValkyrieType::Generic(GenericType { name: Identifier::new("U"), kind: nyar_language::types::hir::HirKind::Type, bounds: vec![] }),
                traits: vec![make_trait_name("Clone")],
                span: None,
            },
        ],
        None,
    );

    solver.add_where_clause(where_clause);

    let result = solver.solve();
    assert!(result.is_ok());
}

#[test]
fn test_lifetime_constraint_outlives() {
    let mut solver = ConstraintSolver::new();

    let constraint = LifetimeConstraint::outlives(Lifetime::new("a"), Lifetime::new("b"), None);

    solver.add_lifetime_constraint(constraint);

    let result = solver.solve();
    assert!(result.is_ok());
}

#[test]
fn test_lifetime_constraint_equality() {
    let mut solver = ConstraintSolver::new();

    let constraint = LifetimeConstraint::equality(Lifetime::new("a"), Lifetime::new("b"), None);

    solver.add_lifetime_constraint(constraint);

    let result = solver.solve();
    assert!(result.is_ok());
}

#[test]
fn test_lifetime_static_check() {
    let mut solver = ConstraintSolver::new();

    solver.add_lifetime_constraint(LifetimeConstraint::outlives(Lifetime::new("a"), Lifetime::static_lifetime(), None));

    let result = solver.solve();
    assert!(result.is_err());
}

#[test]
fn test_propagate_to_caller() {
    let mut solver = ConstraintSolver::new();
    let caller_var = TypeVar(0);

    let callee_constraints = vec![
        TypeConstraint::trait_bound(TypeVar(1), make_trait_name("Display"), None),
        TypeConstraint::trait_bound(TypeVar(1), make_trait_name("Clone"), None),
    ];

    solver.propagate_to_caller(caller_var.clone(), &callee_constraints, Some("test_function".to_string()));

    let suggestions = solver.suggest_fixes();
    assert!(!suggestions.is_empty() || solver.get_substitutions().is_empty());
}

#[test]
fn test_generate_error_report() {
    let solver = ConstraintSolver::new();
    let error = ConstraintError::trait_not_implemented("MyType", "Display", None);

    let report = solver.generate_error_report(&error);

    assert_eq!(report.primary_message, error.message);
    assert!(!report.suggestions.is_empty());
}

#[test]
fn test_generate_error_report_with_type_mismatch() {
    let solver = ConstraintSolver::new();
    let error = ConstraintError::type_mismatch(ValkyrieType::Integer64 { signed: true }, ValkyrieType::Boolean, None);

    let report = solver.generate_error_report(&error);

    assert_eq!(report.related_types.len(), 2);
    assert!(!report.suggestions.is_empty());
}

#[test]
fn test_suggest_fixes() {
    let mut solver = ConstraintSolver::new();
    let var = TypeVar(0);

    solver.add_constraint(TypeConstraint::trait_bound(var.clone(), make_trait_name("Display"), None));
    solver.add_substitution(var, ValkyrieType::Named(Identifier::new("MyType")));

    let suggestions = solver.suggest_fixes();

    assert!(!suggestions.is_empty());
}

#[test]
fn test_constraint_report_detailed_message() {
    let mut report = ConstraintReport::new(
        ConstraintErrorKind::TraitNotImplemented { type_name: "MyType".to_string(), trait_name: "Display".to_string() },
        None,
        "类型 MyType 未实现 Display trait",
    );

    report.add_chain_node(ConstraintChainNode::new("约束来自函数 foo".to_string(), None, Some("foo".to_string())));

    report.add_suggestion(FixSuggestion::new("为 MyType 实现 Display trait", 1).with_code_example("impl Display for MyType { }"));

    let message = report.to_detailed_message();

    assert!(message.contains("错误"));
    assert!(message.contains("约束追溯"));
    assert!(message.contains("建议修复"));
}

#[test]
fn test_fix_suggestion_with_code_example() {
    let suggestion = FixSuggestion::new("添加 trait 实现", 1).with_code_example("impl Display for MyType { }");

    assert!(suggestion.code_example.is_some());
    assert_eq!(suggestion.priority, 1);
}

#[test]
fn test_associated_type_constraint() {
    let mut solver = ConstraintSolver::new();
    let base_type = ValkyrieType::Named(Identifier::new("Counter"));
    let trait_name = make_trait_name("Iterator");
    let assoc_name = Identifier::new("Item");
    let concrete_type = ValkyrieType::Integer64 { signed: true };
    let expected_bound = ValkyrieType::Integer64 { signed: true };

    solver.register_associated_type_impl(base_type.clone(), trait_name.clone(), assoc_name.clone(), concrete_type.clone());

    solver.add_associated_type_constraint(AssociatedTypeConstraint::new(
        base_type.clone(),
        trait_name.clone(),
        assoc_name.clone(),
        expected_bound.clone(),
        None,
    ));

    let result = solver.solve();
    assert!(result.is_ok());
}

#[test]
fn test_associated_type_constraint_mismatch() {
    let mut solver = ConstraintSolver::new();
    let base_type = ValkyrieType::Named(Identifier::new("Counter"));
    let trait_name = make_trait_name("Iterator");
    let assoc_name = Identifier::new("Item");
    let concrete_type = ValkyrieType::Integer64 { signed: true };
    let expected_bound = ValkyrieType::Utf8;

    solver.register_associated_type_impl(base_type.clone(), trait_name.clone(), assoc_name.clone(), concrete_type.clone());

    solver.add_associated_type_constraint(AssociatedTypeConstraint::new(
        base_type.clone(),
        trait_name.clone(),
        assoc_name.clone(),
        expected_bound.clone(),
        None,
    ));

    let result = solver.solve();
    assert!(result.is_err());
}

#[test]
fn test_associated_type_constraint_with_nested_function_and_associated_type() {
    let mut solver = ConstraintSolver::new();
    let base_type = ValkyrieType::Named(Identifier::new("MapperFactory"));
    let trait_name = make_trait_name("Streaming");
    let assoc_name = Identifier::new("Output");
    let concrete_type = ValkyrieType::Function(FunctionType {
        params: vec![ValkyrieType::Integer32 { signed: true }],
        return_type: Box::new(ValkyrieType::Apply(
            Box::new(ValkyrieType::Named(Identifier::new("Boxed"))),
            vec![ValkyrieType::Tuple(vec![
                ValkyrieType::Associated(AssociatedType {
                    base: Box::new(ValkyrieType::Named(Identifier::new("VecIter"))),
                    name: Identifier::new("Item"),
                    type_arguments: vec![ValkyrieType::Integer32 { signed: true }],
                }),
                ValkyrieType::Integer32 { signed: true },
            ])],
        )),
    });

    solver.register_associated_type_impl(base_type.clone(), trait_name.clone(), assoc_name.clone(), concrete_type.clone());
    solver.add_associated_type_constraint(AssociatedTypeConstraint::new(base_type, trait_name, assoc_name, concrete_type, None));

    let result = solver.solve();
    assert!(result.is_ok());
}

#[test]
fn test_associated_type_constraint_with_nested_function_and_associated_type_mismatch() {
    let mut solver = ConstraintSolver::new();
    let base_type = ValkyrieType::Named(Identifier::new("MapperFactory"));
    let trait_name = make_trait_name("Streaming");
    let assoc_name = Identifier::new("Output");

    solver.register_associated_type_impl(
        base_type.clone(),
        trait_name.clone(),
        assoc_name.clone(),
        ValkyrieType::Function(FunctionType {
            params: vec![ValkyrieType::Integer32 { signed: true }],
            return_type: Box::new(ValkyrieType::Apply(
                Box::new(ValkyrieType::Named(Identifier::new("Boxed"))),
                vec![ValkyrieType::Tuple(vec![
                    ValkyrieType::Associated(AssociatedType {
                        base: Box::new(ValkyrieType::Named(Identifier::new("VecIter"))),
                        name: Identifier::new("Item"),
                        type_arguments: vec![ValkyrieType::Integer32 { signed: true }],
                    }),
                    ValkyrieType::Integer32 { signed: true },
                ])],
            )),
        }),
    );

    solver.add_associated_type_constraint(AssociatedTypeConstraint::new(
        base_type,
        trait_name,
        assoc_name,
        ValkyrieType::Function(FunctionType {
            params: vec![ValkyrieType::Integer32 { signed: true }],
            return_type: Box::new(ValkyrieType::Apply(
                Box::new(ValkyrieType::Named(Identifier::new("Boxed"))),
                vec![ValkyrieType::Tuple(vec![
                    ValkyrieType::Associated(AssociatedType {
                        base: Box::new(ValkyrieType::Named(Identifier::new("VecIter"))),
                        name: Identifier::new("Item"),
                        type_arguments: vec![ValkyrieType::Integer64 { signed: true }],
                    }),
                    ValkyrieType::Integer32 { signed: true },
                ])],
            )),
        }),
        None,
    ));

    let result = solver.solve();
    assert!(result.is_err());
}

#[test]
fn test_where_clause_uses_binding_from_associated_type_constraint() {
    let mut solver = ConstraintSolver::new();
    let generic_t = ValkyrieType::Generic(GenericType { name: Identifier::new("T"), kind: nyar_language::types::hir::HirKind::Type, bounds: vec![] });

    solver.register_associated_type_impl(
        ValkyrieType::Named(Identifier::new("MapperFactory")),
        make_trait_name("Streaming"),
        Identifier::new("Output"),
        ValkyrieType::Function(FunctionType {
            params: vec![ValkyrieType::Integer32 { signed: true }],
            return_type: Box::new(ValkyrieType::Apply(
                Box::new(ValkyrieType::Named(Identifier::new("Boxed"))),
                vec![ValkyrieType::Tuple(vec![ValkyrieType::Integer64 { signed: true }, ValkyrieType::Integer64 { signed: true }])],
            )),
        }),
    );

    solver.add_associated_type_constraint(AssociatedTypeConstraint::new(
        ValkyrieType::Named(Identifier::new("MapperFactory")),
        make_trait_name("Streaming"),
        Identifier::new("Output"),
        ValkyrieType::Function(FunctionType {
            params: vec![ValkyrieType::Integer32 { signed: true }],
            return_type: Box::new(ValkyrieType::Apply(
                Box::new(ValkyrieType::Named(Identifier::new("Boxed"))),
                vec![ValkyrieType::Tuple(vec![generic_t.clone(), generic_t.clone()])],
            )),
        }),
        None,
    ));

    solver.add_where_clause(WhereClause::new(
        vec![WhereBound { ty: generic_t.clone(), traits: vec![make_trait_name("Clone")], span: None }],
        None,
    ));

    let result = solver.solve();
    assert!(result.is_ok());
    assert_eq!(solver.generic_bindings().get(&Identifier::new("T")), Some(&ValkyrieType::Integer64 { signed: true }));
}

#[test]
fn test_where_clause_reports_trait_error_after_associated_type_binding() {
    let mut solver = ConstraintSolver::new();
    let generic_t = ValkyrieType::Generic(GenericType { name: Identifier::new("T"), kind: nyar_language::types::hir::HirKind::Type, bounds: vec![] });

    solver.register_associated_type_impl(
        ValkyrieType::Named(Identifier::new("MapperFactory")),
        make_trait_name("Streaming"),
        Identifier::new("Output"),
        ValkyrieType::Function(FunctionType {
            params: vec![ValkyrieType::Integer32 { signed: true }],
            return_type: Box::new(ValkyrieType::Apply(
                Box::new(ValkyrieType::Named(Identifier::new("Boxed"))),
                vec![ValkyrieType::Tuple(vec![ValkyrieType::Integer64 { signed: true }, ValkyrieType::Integer64 { signed: true }])],
            )),
        }),
    );

    solver.add_associated_type_constraint(AssociatedTypeConstraint::new(
        ValkyrieType::Named(Identifier::new("MapperFactory")),
        make_trait_name("Streaming"),
        Identifier::new("Output"),
        ValkyrieType::Function(FunctionType {
            params: vec![ValkyrieType::Integer32 { signed: true }],
            return_type: Box::new(ValkyrieType::Apply(
                Box::new(ValkyrieType::Named(Identifier::new("Boxed"))),
                vec![ValkyrieType::Tuple(vec![generic_t.clone(), generic_t.clone()])],
            )),
        }),
        None,
    ));

    solver.add_where_clause(WhereClause::new(vec![WhereBound { ty: generic_t, traits: vec![make_trait_name("Display")], span: None }], None));

    let result = solver.solve();
    assert!(result.is_err());
    if let Err(error) = result {
        assert!(matches!(error.kind, ConstraintErrorKind::TraitNotImplemented { .. }));
        assert!(error.message.contains("Display"));
    }
}

#[test]
fn test_trait_bound_checker() {
    let mut checker = TraitBoundChecker::new();

    assert!(checker.is_builtin_trait(&make_trait_name("Clone")));
    assert!(checker.is_builtin_trait(&make_trait_name("Copy")));
    assert!(!checker.is_builtin_trait(&make_trait_name("CustomTrait")));

    checker.register_trait_impl(TraitImpl {
        ty: ValkyrieType::Named(Identifier::new("MyType")),
        trait_name: make_trait_name("CustomTrait"),
        type_args: vec![],
    });

    assert!(checker.check_trait_bound(&ValkyrieType::Named(Identifier::new("MyType")), &make_trait_name("CustomTrait")).unwrap());
}

#[test]
fn test_constraint_propagator() {
    let mut propagator = ConstraintPropagator::new();
    let var = TypeVar(0);

    propagator.propagate_to_caller(
        var.clone(),
        &[TypeConstraint::trait_bound(TypeVar(1), make_trait_name("Display"), None)],
        Some("test".to_string()),
    );

    propagator.compute_transitive_closure().unwrap();

    assert!(propagator.get_trait_bounds(&var).is_some());
}

#[test]
fn test_where_clause_empty() {
    let clause = WhereClause::empty();
    assert!(clause.is_empty());

    let mut clause = WhereClause::empty();
    clause.add_bound(WhereBound {
        ty: ValkyrieType::Generic(GenericType { name: Identifier::new("T"), kind: nyar_language::types::hir::HirKind::Type, bounds: vec![] }),
        traits: vec![make_trait_name("Display")],
        span: None,
    });

    assert!(!clause.is_empty());
}

#[test]
fn test_multi_trait_bound_empty() {
    let bound = MultiTraitBound::new(TypeVar(0), vec![], None);
    assert!(bound.is_empty());

    let bound = MultiTraitBound::new(TypeVar(0), vec![make_trait_name("Display")], None);
    assert!(!bound.is_empty());
}

#[test]
fn test_associated_type_not_found_error() {
    let error = ConstraintError::associated_type_not_found(
        ValkyrieType::Named(Identifier::new("MyType")),
        make_trait_name("Iterator"),
        Identifier::new("Item"),
        None,
    );

    assert!(error.message.contains("关联类型未找到"));
}

#[test]
fn test_propagation_failed_error() {
    let error = ConstraintError::propagation_failed("测试原因", None);
    assert!(error.message.contains("约束传播失败"));
}

#[test]
fn test_resolve_associated_type() {
    let mut solver = ConstraintSolver::new();
    let base_type = ValkyrieType::Named(Identifier::new("Counter"));
    let trait_name = make_trait_name("Iterator");
    let assoc_name = Identifier::new("Item");
    let concrete_type = ValkyrieType::Integer64 { signed: true };

    solver.register_associated_type_impl(base_type.clone(), trait_name.clone(), assoc_name.clone(), concrete_type.clone());

    let result = solver.resolve_associated_type(&base_type, &trait_name, &assoc_name);
    assert_eq!(result, Some(concrete_type));
}

#[test]
fn test_unify_associated_types() {
    let mut solver = ConstraintSolver::new();
    let ty = ValkyrieType::Named(Identifier::new("Counter"));
    let assoc_name = Identifier::new("Item");

    let assoc_ty1 = ValkyrieType::Associated(AssociatedType { base: Box::new(ty.clone()), name: assoc_name.clone(), type_arguments: vec![] });
    let assoc_ty2 = ValkyrieType::Associated(AssociatedType { base: Box::new(ty), name: assoc_name, type_arguments: vec![] });

    let result = solver.unify(&assoc_ty1, &assoc_ty2, None);
    assert!(result.is_ok());
}

#[test]
fn test_unify_associated_types_different_names() {
    let mut solver = ConstraintSolver::new();
    let ty = ValkyrieType::Named(Identifier::new("Counter"));

    let assoc_ty1 =
        ValkyrieType::Associated(AssociatedType { base: Box::new(ty.clone()), name: Identifier::new("Item"), type_arguments: vec![] });
    let assoc_ty2 = ValkyrieType::Associated(AssociatedType { base: Box::new(ty), name: Identifier::new("Value"), type_arguments: vec![] });

    let result = solver.unify(&assoc_ty1, &assoc_ty2, None);
    assert!(matches!(result, Err(ConstraintError { .. })));
}

#[test]
fn test_unity_variant_subtype() {
    use nyar_language::types::hir::{GenericType, HirEnum, HirKind, HirVariant, HirVisibility};

    let mut solver = ConstraintSolver::new();

    let option_enum = HirEnum {
        name: Identifier::new("Option"),
        is_unity: true,
        generics: vec![GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] }],
        variants: vec![
            HirVariant { name: Identifier::new("Some"), doc: Default::default(), fields: vec![], result_type: None, discriminator: None },
            HirVariant { name: Identifier::new("None"), doc: Default::default(), fields: vec![], result_type: None, discriminator: None },
        ],
        doc: Default::default(),
        visibility: HirVisibility::public(),
    };

    solver.register_unity_type(&option_enum);

    let some_type = ValkyrieType::Named(Identifier::new("Some"));
    let option_type = ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Option"))), vec![ValkyrieType::Integer32 { signed: true }]);

    assert!(solver.is_subtype(&some_type, &option_type));

    let none_type = ValkyrieType::Named(Identifier::new("None"));
    assert!(solver.is_subtype(&none_type, &option_type));
}

#[test]
fn test_gadt_variant_result_type_subtype() {
    use nyar_language::types::hir::{GenericType, HirEnum, HirField, HirKind, HirVariant, HirVisibility};

    let mut solver = ConstraintSolver::new();

    let expr_enum = HirEnum {
        name: Identifier::new("Expr"),
        is_unity: true,
        generics: vec![GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] }],
        variants: vec![
            HirVariant {
                name: Identifier::new("Literal"),
                doc: Default::default(),
                fields: vec![HirField {
                    name: Identifier::new("value"),
                    doc: Default::default(),
                    ty: ValkyrieType::Float64,
                    visibility: HirVisibility::public(),
                    is_mutable: false,
                }],
                result_type: Some(ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Expr"))), vec![ValkyrieType::Float64])),
                discriminator: None,
            },
            HirVariant {
                name: Identifier::new("If"),
                doc: Default::default(),
                fields: vec![],
                result_type: Some(ValkyrieType::Apply(
                    Box::new(ValkyrieType::Named(Identifier::new("Expr"))),
                    vec![ValkyrieType::Generic(GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] })],
                )),
                discriminator: None,
            },
        ],
        doc: Default::default(),
        visibility: HirVisibility::public(),
    };

    solver.register_unity_type(&expr_enum);

    assert!(solver.is_subtype(
        &ValkyrieType::Named(Identifier::new("Literal")),
        &ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Expr"))), vec![ValkyrieType::Float64]),
    ));
    assert!(!solver.is_subtype(
        &ValkyrieType::Named(Identifier::new("Literal")),
        &ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Expr"))), vec![ValkyrieType::Integer64 { signed: true }]),
    ));
    assert!(solver.is_subtype(
        &ValkyrieType::Named(Identifier::new("If")),
        &ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Expr"))), vec![ValkyrieType::Integer32 { signed: true }]),
    ));
}

#[test]
fn test_gadt_variant_result_type_matches_nested_tuple_pattern() {
    use nyar_language::types::hir::{GenericType, HirEnum, HirKind, HirVariant, HirVisibility};

    let mut solver = ConstraintSolver::new();

    let pair_expr_enum = HirEnum {
        name: Identifier::new("PairExpr"),
        is_unity: true,
        generics: vec![GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] }],
        variants: vec![HirVariant {
            name: Identifier::new("PairValue"),
            doc: Default::default(),
            fields: vec![],
            result_type: Some(ValkyrieType::Apply(
                Box::new(ValkyrieType::Named(Identifier::new("PairExpr"))),
                vec![ValkyrieType::Tuple(vec![
                    ValkyrieType::Generic(GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] }),
                    ValkyrieType::Generic(GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] }),
                ])],
            )),
            discriminator: None,
        }],
        doc: Default::default(),
        visibility: HirVisibility::public(),
    };

    solver.register_unity_type(&pair_expr_enum);

    assert!(solver.is_subtype(
        &ValkyrieType::Named(Identifier::new("PairValue")),
        &ValkyrieType::Apply(
            Box::new(ValkyrieType::Named(Identifier::new("PairExpr"))),
            vec![ValkyrieType::Tuple(vec![ValkyrieType::Integer32 { signed: true }, ValkyrieType::Integer32 { signed: true }])],
        ),
    ));
    assert!(!solver.is_subtype(
        &ValkyrieType::Named(Identifier::new("PairValue")),
        &ValkyrieType::Apply(
            Box::new(ValkyrieType::Named(Identifier::new("PairExpr"))),
            vec![ValkyrieType::Tuple(vec![ValkyrieType::Integer32 { signed: true }, ValkyrieType::Integer64 { signed: true }])],
        ),
    ));
}

#[test]
fn test_gadt_variant_result_type_matches_function_with_nested_apply_tuple_return() {
    use nyar_language::types::hir::{GenericType, HirEnum, HirKind, HirVariant, HirVisibility};

    let mut solver = ConstraintSolver::new();

    let callable_enum = HirEnum {
        name: Identifier::new("Callable"),
        is_unity: true,
        generics: vec![GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] }],
        variants: vec![HirVariant {
            name: Identifier::new("Duplicator"),
            doc: Default::default(),
            fields: vec![],
            result_type: Some(ValkyrieType::Apply(
                Box::new(ValkyrieType::Named(Identifier::new("Callable"))),
                vec![ValkyrieType::Function(FunctionType {
                    params: vec![ValkyrieType::Generic(GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] })],
                    return_type: Box::new(ValkyrieType::Apply(
                        Box::new(ValkyrieType::Named(Identifier::new("Boxed"))),
                        vec![ValkyrieType::Tuple(vec![
                            ValkyrieType::Generic(GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] }),
                            ValkyrieType::Generic(GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] }),
                        ])],
                    )),
                })],
            )),
            discriminator: None,
        }],
        doc: Default::default(),
        visibility: HirVisibility::public(),
    };

    solver.register_unity_type(&callable_enum);

    assert!(solver.is_subtype(
        &ValkyrieType::Named(Identifier::new("Duplicator")),
        &ValkyrieType::Apply(
            Box::new(ValkyrieType::Named(Identifier::new("Callable"))),
            vec![ValkyrieType::Function(FunctionType {
                params: vec![ValkyrieType::Integer32 { signed: true }],
                return_type: Box::new(ValkyrieType::Apply(
                    Box::new(ValkyrieType::Named(Identifier::new("Boxed"))),
                    vec![ValkyrieType::Tuple(vec![ValkyrieType::Integer32 { signed: true }, ValkyrieType::Integer32 { signed: true }])],
                )),
            })],
        ),
    ));
    assert!(!solver.is_subtype(
        &ValkyrieType::Named(Identifier::new("Duplicator")),
        &ValkyrieType::Apply(
            Box::new(ValkyrieType::Named(Identifier::new("Callable"))),
            vec![ValkyrieType::Function(FunctionType {
                params: vec![ValkyrieType::Integer32 { signed: true }],
                return_type: Box::new(ValkyrieType::Apply(
                    Box::new(ValkyrieType::Named(Identifier::new("Boxed"))),
                    vec![ValkyrieType::Tuple(vec![ValkyrieType::Integer32 { signed: true }, ValkyrieType::Integer64 { signed: true }])],
                )),
            })],
        ),
    ));
    assert!(!solver.is_subtype(
        &ValkyrieType::Named(Identifier::new("Duplicator")),
        &ValkyrieType::Apply(
            Box::new(ValkyrieType::Named(Identifier::new("Callable"))),
            vec![ValkyrieType::Function(FunctionType {
                params: vec![ValkyrieType::Integer64 { signed: true }],
                return_type: Box::new(ValkyrieType::Apply(
                    Box::new(ValkyrieType::Named(Identifier::new("Boxed"))),
                    vec![ValkyrieType::Tuple(vec![ValkyrieType::Integer32 { signed: true }, ValkyrieType::Integer32 { signed: true }])],
                )),
            })],
        ),
    ));
}

#[test]
fn test_gadt_variant_result_type_matches_function_with_nested_associated_type() {
    use nyar_language::types::hir::{GenericType, HirEnum, HirKind, HirVariant, HirVisibility};

    let mut solver = ConstraintSolver::new();

    let stream_callable_enum = HirEnum {
        name: Identifier::new("StreamCallable"),
        is_unity: true,
        generics: vec![
            GenericType { name: Identifier::new("Iter"), kind: HirKind::Type, bounds: vec![] },
            GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] },
        ],
        variants: vec![HirVariant {
            name: Identifier::new("NextMapper"),
            doc: Default::default(),
            fields: vec![],
            result_type: Some(ValkyrieType::Apply(
                Box::new(ValkyrieType::Named(Identifier::new("StreamCallable"))),
                vec![
                    ValkyrieType::Generic(GenericType { name: Identifier::new("Iter"), kind: HirKind::Type, bounds: vec![] }),
                    ValkyrieType::Function(FunctionType {
                        params: vec![ValkyrieType::Generic(GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] })],
                        return_type: Box::new(ValkyrieType::Apply(
                            Box::new(ValkyrieType::Named(Identifier::new("Boxed"))),
                            vec![ValkyrieType::Tuple(vec![
                                ValkyrieType::Associated(AssociatedType {
                                    base: Box::new(ValkyrieType::Generic(GenericType {
                                        name: Identifier::new("Iter"),
                                        kind: HirKind::Type,
                                        bounds: vec![],
                                    })),
                                    name: Identifier::new("Item"),
                                    type_arguments: vec![ValkyrieType::Generic(GenericType {
                                        name: Identifier::new("T"),
                                        kind: HirKind::Type,
                                        bounds: vec![],
                                    })],
                                }),
                                ValkyrieType::Generic(GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] }),
                            ])],
                        )),
                    }),
                ],
            )),
            discriminator: None,
        }],
        doc: Default::default(),
        visibility: HirVisibility::public(),
    };

    solver.register_unity_type(&stream_callable_enum);

    assert!(solver.is_subtype(
        &ValkyrieType::Named(Identifier::new("NextMapper")),
        &ValkyrieType::Apply(
            Box::new(ValkyrieType::Named(Identifier::new("StreamCallable"))),
            vec![
                ValkyrieType::Named(Identifier::new("VecIter")),
                ValkyrieType::Function(FunctionType {
                    params: vec![ValkyrieType::Integer32 { signed: true }],
                    return_type: Box::new(ValkyrieType::Apply(
                        Box::new(ValkyrieType::Named(Identifier::new("Boxed"))),
                        vec![ValkyrieType::Tuple(vec![
                            ValkyrieType::Associated(AssociatedType {
                                base: Box::new(ValkyrieType::Named(Identifier::new("VecIter"))),
                                name: Identifier::new("Item"),
                                type_arguments: vec![ValkyrieType::Integer32 { signed: true }],
                            }),
                            ValkyrieType::Integer32 { signed: true },
                        ])],
                    )),
                }),
            ],
        ),
    ));
    assert!(!solver.is_subtype(
        &ValkyrieType::Named(Identifier::new("NextMapper")),
        &ValkyrieType::Apply(
            Box::new(ValkyrieType::Named(Identifier::new("StreamCallable"))),
            vec![
                ValkyrieType::Named(Identifier::new("VecIter")),
                ValkyrieType::Function(FunctionType {
                    params: vec![ValkyrieType::Integer32 { signed: true }],
                    return_type: Box::new(ValkyrieType::Apply(
                        Box::new(ValkyrieType::Named(Identifier::new("Boxed"))),
                        vec![ValkyrieType::Tuple(vec![
                            ValkyrieType::Associated(AssociatedType {
                                base: Box::new(ValkyrieType::Named(Identifier::new("OtherIter"))),
                                name: Identifier::new("Item"),
                                type_arguments: vec![ValkyrieType::Integer32 { signed: true }],
                            }),
                            ValkyrieType::Integer32 { signed: true },
                        ])],
                    )),
                }),
            ],
        ),
    ));
    assert!(!solver.is_subtype(
        &ValkyrieType::Named(Identifier::new("NextMapper")),
        &ValkyrieType::Apply(
            Box::new(ValkyrieType::Named(Identifier::new("StreamCallable"))),
            vec![
                ValkyrieType::Named(Identifier::new("VecIter")),
                ValkyrieType::Function(FunctionType {
                    params: vec![ValkyrieType::Integer32 { signed: true }],
                    return_type: Box::new(ValkyrieType::Apply(
                        Box::new(ValkyrieType::Named(Identifier::new("Boxed"))),
                        vec![ValkyrieType::Tuple(vec![
                            ValkyrieType::Associated(AssociatedType {
                                base: Box::new(ValkyrieType::Named(Identifier::new("VecIter"))),
                                name: Identifier::new("Item"),
                                type_arguments: vec![ValkyrieType::Integer64 { signed: true }],
                            }),
                            ValkyrieType::Integer32 { signed: true },
                        ])],
                    )),
                }),
            ],
        ),
    ));
}

#[test]
fn test_unity_variant_subtype_with_type_args() {
    use nyar_language::types::hir::{GenericType, HirEnum, HirKind, HirVariant, HirVisibility};

    let mut solver = ConstraintSolver::new();

    let result_enum = HirEnum {
        name: Identifier::new("Result"),
        is_unity: true,
        generics: vec![
            GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] },
            GenericType { name: Identifier::new("E"), kind: HirKind::Type, bounds: vec![] },
        ],
        variants: vec![
            HirVariant { name: Identifier::new("Fine"), doc: Default::default(), fields: vec![], result_type: None, discriminator: None },
            HirVariant { name: Identifier::new("Fail"), doc: Default::default(), fields: vec![], result_type: None, discriminator: None },
        ],
        doc: Default::default(),
        visibility: HirVisibility::public(),
    };

    solver.register_unity_type(&result_enum);

    let fine_type = ValkyrieType::Named(Identifier::new("Fine"));
    let result_type =
        ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Result"))), vec![ValkyrieType::Integer32 { signed: true }, ValkyrieType::Utf8]);

    assert!(solver.is_subtype(&fine_type, &result_type));

    let fail_type = ValkyrieType::Named(Identifier::new("Fail"));
    assert!(solver.is_subtype(&fail_type, &result_type));
}

#[test]
fn test_unity_variant_not_subtype_of_different_unity() {
    use nyar_language::types::hir::{GenericType, HirEnum, HirKind, HirVariant, HirVisibility};

    let mut solver = ConstraintSolver::new();

    let option_enum = HirEnum {
        name: Identifier::new("Option"),
        is_unity: true,
        generics: vec![GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] }],
        variants: vec![
            HirVariant { name: Identifier::new("Some"), doc: Default::default(), fields: vec![], result_type: None, discriminator: None },
            HirVariant { name: Identifier::new("None"), doc: Default::default(), fields: vec![], result_type: None, discriminator: None },
        ],
        doc: Default::default(),
        visibility: HirVisibility::public(),
    };

    let result_enum = HirEnum {
        name: Identifier::new("Result"),
        is_unity: true,
        generics: vec![
            GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] },
            GenericType { name: Identifier::new("E"), kind: HirKind::Type, bounds: vec![] },
        ],
        variants: vec![
            HirVariant { name: Identifier::new("Fine"), doc: Default::default(), fields: vec![], result_type: None, discriminator: None },
            HirVariant { name: Identifier::new("Fail"), doc: Default::default(), fields: vec![], result_type: None, discriminator: None },
        ],
        doc: Default::default(),
        visibility: HirVisibility::public(),
    };

    solver.register_unity_type(&option_enum);
    solver.register_unity_type(&result_enum);

    let some_type = ValkyrieType::Named(Identifier::new("Some"));
    let result_type =
        ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Result"))), vec![ValkyrieType::Integer32 { signed: true }, ValkyrieType::Utf8]);

    assert!(!solver.is_subtype(&some_type, &result_type));
}

#[test]
fn test_unity_subtype_constraint() {
    use nyar_language::types::hir::{GenericType, HirEnum, HirKind, HirVariant, HirVisibility};

    let mut solver = ConstraintSolver::new();

    let option_enum = HirEnum {
        name: Identifier::new("Option"),
        is_unity: true,
        generics: vec![GenericType { name: Identifier::new("T"), kind: HirKind::Type, bounds: vec![] }],
        variants: vec![
            HirVariant { name: Identifier::new("Some"), doc: Default::default(), fields: vec![], result_type: None, discriminator: None },
            HirVariant { name: Identifier::new("None"), doc: Default::default(), fields: vec![], result_type: None, discriminator: None },
        ],
        doc: Default::default(),
        visibility: HirVisibility::public(),
    };

    solver.register_unity_type(&option_enum);

    let some_type = ValkyrieType::Named(Identifier::new("Some"));
    let option_type = ValkyrieType::Apply(Box::new(ValkyrieType::Named(Identifier::new("Option"))), vec![ValkyrieType::Integer32 { signed: true }]);

    solver.add_constraint(TypeConstraint::subtype(some_type, option_type, None));

    let result = solver.solve();
    assert!(result.is_ok());
}
