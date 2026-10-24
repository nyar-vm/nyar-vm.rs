//! 在调用解析前将 trait 路径冻结到 Compiler 的声明身份。
use super::*;
use nyar_types::ItemId;
use std::collections::BTreeMap;

pub(super) fn bind_trait_declarations(module: &mut HirModule) -> Result<(), ParseError> {
    let mut declarations = BTreeMap::<NamePath, Vec<(ItemId, usize)>>::new();
    fn collect_trait(item: &HirTrait, declarations: &mut BTreeMap<NamePath, Vec<(ItemId, usize)>>) -> Result<(), ParseError> {
        let identity = item.declaration.ok_or_else(|| ParseError::Invalid { message: "trait export 缺少声明身份".into(), span: None })?;
        let mut path = item.declaring_namespace.parts().to_vec();
        path.push(item.name.clone());
        let entries = declarations.entry(NamePath::new(path)).or_default();
        if !entries.iter().any(|entry| entry.0 == identity) {
            entries.push((identity, item.generics.len()));
        }
        Ok(())
    }
    fn collect(module: &HirModule, declarations: &mut BTreeMap<NamePath, Vec<(ItemId, usize)>>) -> Result<(), ParseError> {
        for item in &module.traits {
            collect_trait(item, declarations)?;
        }
        for export in &module.imported_semantic_exports {
            for item in &export.traits {
                collect_trait(item, declarations)?;
            }
        }
        for child in &module.submodules {
            collect(child, declarations)?;
        }
        Ok(())
    }
    collect(module, &mut declarations)?;
    fn lookup(
        path: &NamePath,
        namespace: &NamePath,
        declarations: &BTreeMap<NamePath, Vec<(ItemId, usize)>>,
    ) -> Result<(ItemId, usize), ParseError> {
        // 限定路径按整段绑定；裸名只按声明作用域逐级查找。
        let mut candidates = Vec::new();
        if path.parts().len() > 1 {
            candidates.push(path.clone());
        }
        else {
            for depth in (0..=namespace.parts().len()).rev() {
                let mut full = namespace.parts()[..depth].to_vec();
                full.extend_from_slice(path.parts());
                candidates.push(NamePath::new(full));
            }
        }
        for candidate in candidates {
            if let Some(entries) = declarations.get(&candidate) {
                if entries.len() != 1 {
                    return Err(ParseError::Invalid { message: format!("trait 声明歧义：{path}"), span: None });
                }
                return Ok(entries[0]);
            }
        }
        // 裸名在声明作用域链未命中时，只在本模块可见的声明集合内按唯一短名回退。
        // 该集合由「本组声明 + `direct_dependencies` 导出」构成，与语义闭包的可见性合同
        // 是同一份事实；`std.text` 的 `imply AsciiText: Text` 命中 `core::text::Text` 即走这里。
        // 这不是跨全库的名字回退：不可见组的声明根本不在 `declarations` 中。
        if let Some(name) = path.parts().last() {
            let mut matched: Option<(ItemId, usize)> = None;
            for (declared, entries) in declarations {
                if declared.parts().last() != Some(name) {
                    continue;
                }
                for entry in entries {
                    match matched {
                        None => matched = Some(*entry),
                        Some(existing) if existing.0 == entry.0 => {}
                        Some(_) => return Err(ParseError::Invalid { message: format!("trait 声明歧义：{path}"), span: None }),
                    }
                }
            }
            if let Some(entry) = matched {
                return Ok(entry);
            }
        }
        Err(ParseError::Invalid { message: format!("trait 约束未解析：{path}"), span: None })
    }
    fn bounds(
        constraints: &mut [HirWhereConstraint],
        namespace: &NamePath,
        declarations: &BTreeMap<NamePath, Vec<(ItemId, usize)>>,
    ) -> Result<(), ParseError> {
        for constraint in constraints {
            for bound in &mut constraint.bounds {
                let (identity, arity) = lookup(&bound.trait_path, namespace, declarations)?;
                if bound.type_arguments.len() != arity {
                    return Err(ParseError::Invalid { message: format!("trait 类型实参数量不符：{}", bound.trait_path), span: None });
                }
                bound.declaration = Some(identity);
            }
        }
        Ok(())
    }
    fn function(
        item: &mut HirFunction,
        namespace: &NamePath,
        declarations: &BTreeMap<NamePath, Vec<(ItemId, usize)>>,
    ) -> Result<(), ParseError> {
        bounds(&mut item.where_constraints, namespace, declarations)
    }
    fn bind(module: &mut HirModule, declarations: &BTreeMap<NamePath, Vec<(ItemId, usize)>>) -> Result<(), ParseError> {
        for item in &mut module.functions {
            let namespace = item.declaring_namespace.clone();
            function(item, &namespace, declarations)?;
        }
        for item in &mut module.structs {
            let namespace = NamePath::new(item.namespace.clone());
            for method in &mut item.methods {
                function(method, &namespace, declarations)?;
            }
        }
        for item in &mut module.traits {
            for method in item.methods.iter_mut().chain(item.default_methods.iter_mut()) {
                function(method, &item.declaring_namespace, declarations)?;
            }
        }
        for item in &mut module.impls {
            if let Some(path) = &item.trait_path {
                let (identity, arity) = lookup(path, &item.declaring_namespace, declarations)?;
                if arity != item.trait_arguments.len() {
                    return Err(ParseError::Invalid { message: format!("imply trait 类型实参数量不符：{path}"), span: None });
                }
                item.trait_declaration = Some(identity);
            }
            bounds(&mut item.where_constraints, &item.declaring_namespace, declarations)?;
            for method in &mut item.methods {
                function(method, &item.declaring_namespace, declarations)?;
            }
        }
        for item in &mut module.singletons {
            let namespace = NamePath::new(item.namespace.clone());
            for method in
                item.methods.iter_mut().chain(item.constructor.iter_mut().map(Box::as_mut)).chain(item.finalizer.iter_mut().map(Box::as_mut))
            {
                function(method, &namespace, declarations)?;
            }
        }
        for child in &mut module.submodules {
            bind(child, declarations)?;
        }
        Ok(())
    }
    bind(module, &declarations)
}
