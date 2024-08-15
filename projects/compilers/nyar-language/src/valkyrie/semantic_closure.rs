//! 入口驱动的语义闭包规划：可见性合同下的声明图、body lowering 边界与内容摘要。
//!
//! 闭包是「目标模块能看见的声明」的上界，由声明身份与依赖可见性驱动，而不是文件短名近似：
//!
//! - 可见性的事实来源是 Resolver 给出的 `direct_dependencies`：一条引用只能命中本组声明
//!   或本组直接依赖的声明。闭包不做跨组全局名字回退，也不把整个依赖包的 `.v` 文件拉进闭包；
//! - 种子是入口组的声明；依赖组只在自身声明被需求时进入 body lowering；
//! - 自由名引用按「限定路径 → 当前 namespace → 父 namespace」解析；链上未命中的单段名
//!   只在可见组内按短名回退，且只接受非成员声明：方法/imply 成员没有名字回退索引；
//! - 成员选择名不建立全库同名索引：限定形式 `<聚合>.<成员>` 先解析聚合身份再取成员身份；
//!   接收者类型在索引阶段不可知的选择名，只在该名字确实出现在闭包需求中、且其拥有聚合
//!   已被需求时，作为**该聚合的成员**进入闭包；
//! - 运算符是普通成员调用：`a + b` / `!a` / `a[i]` 登记为 `infix +` / `prefix !` / `suffix []`
//!   的成员引用，成员名与 HIR lowering 共用 `operator_members` 这一唯一映射；
//! - 闭包只做上界判定，不做歧义判定：精确调用解析与歧义归 HIR；
//! - 摘要绑定规划版本、依赖身份、被编译声明身份与这些声明的源文件内容。

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use nyar_types::combined_content_hash;
use oak_valkyrie::lexer::token_type::ValkyrieTokenType;

use crate::valkyrie::{
    frontend::{ast, parse_source},
    operator_members,
};

/// 闭包规划版本；规划语义变化必须递增，旧摘要不得跨版本复用。
const CLOSURE_PLANNER_VERSION: &str = "semantic-closure-v1";

// ---------------------------------------------------------------------------
// Declaration 索引中立数据结构（生产权在 nyar-language frontend inventory）
// 定义在 nyar_types::compiler_input，本模块 re-export 供内部使用。
// ---------------------------------------------------------------------------
pub use nyar_types::compiler_input::{
    DeclarationIndexEntry, DeclarationKey, DeclarationKind, DeclarationLink, EntryPointKind, ReferencePath,
};

/// namespace 段 + 文件名路径引用的组合（文件级语句所属 namespace 与它引用的名字）。
type ScopedReference = (Vec<String>, ReferencePath);

/// 单组语义闭包切片。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupSemanticClosure {
    /// 依赖组身份。
    pub dependency_key: String,
    /// 进入 body lowering 的仓相对路径。
    pub body_lowering_source_paths: BTreeSet<String>,
    /// 本组 body lowering 覆盖的声明身份。
    pub body_lowering_declarations: BTreeSet<DeclarationKey>,
}

/// 入口驱动的完整语义闭包：声明索引 + body lowering 边界 + 内容摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticClosure {
    /// 各依赖组的 body lowering 切片。
    pub groups: Vec<GroupSemanticClosure>,
    /// 全 inventory 声明索引（含不可达文件）。
    pub declaration_index: BTreeMap<DeclarationKey, DeclarationIndexEntry>,
    /// 入口闭包内被引用的声明身份。
    pub demanded_declarations: BTreeSet<DeclarationKey>,
    /// 未能建立声明索引的 inventory 文件（`依赖组::路径` → 失败原因）。
    pub unindexed_source_files: BTreeMap<String, String>,
    /// 闭包稳定摘要。
    pub digest: String,
}

/// 单个源码组的闭包规划输入：组身份、模块名、直接依赖身份、是否入口组、组内文件。
///
/// `direct_dependencies` 是可见性合同的唯一事实来源：它决定本组引用可以命中哪些组的声明。
pub struct ClosureGroupInput<'a> {
    /// 依赖组身份。
    pub dependency_key: &'a str,
    /// 语义组模块名；无 namespace 的声明用它形成与 HIR 稳定符号一致的限定名。
    pub module_name: &'a str,
    /// 本组可直接引用的依赖组身份。
    pub direct_dependencies: &'a [&'a str],
    /// 是否为唯一入口组。
    pub is_entry: bool,
    /// 组内文件（仓相对路径、源码）。
    pub files: &'a [(String, String)],
}

/// 语义闭包规划的结构化失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticClosureError {
    /// 入口组没有提供任何 `[export]` / `[main]` 声明：不得把整组文件当作可执行程序。
    MissingEntryDeclaration {
        /// 失败入口组身份。
        dependency_key: String,
        /// 入口组内未能建立索引的文件（路径、原因）；为空表示文件可解析但没有入口注解。
        unindexed_source_files: Vec<(String, String)>,
    },
    /// Resolver 选定的 provider 符号没有命中唯一声明身份；被需求合同不得缺实现。
    HostProviderNotResolved {
        /// 绑定中的 provider 稳定符号。
        symbol: String,
    },
    /// 同一 provider 符号命中多个声明身份：不得择一继续。
    HostProviderAmbiguous {
        /// 绑定中的 provider 稳定符号。
        symbol: String,
        /// 命中的声明身份（依赖组::路径::种类::namespace::拥有者::名字）。
        declarations: Vec<String>,
    },
}

impl std::fmt::Display for SemanticClosureError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingEntryDeclaration { dependency_key, unindexed_source_files } => {
                write!(formatter, "entry source group `{dependency_key}` declares no `[export]` or `[main]` entry")?;
                if !unindexed_source_files.is_empty() {
                    write!(formatter, "; unindexed sources:")?;
                    for (path, reason) in unindexed_source_files {
                        write!(formatter, " {path}: {reason};")?;
                    }
                }
                Ok(())
            }
            Self::HostProviderNotResolved { symbol } => {
                write!(formatter, "host provider `{symbol}` 未在语义源码闭包中解析到唯一声明身份")
            }
            Self::HostProviderAmbiguous { symbol, declarations } => {
                write!(formatter, "host provider `{symbol}` 在语义源码闭包中命中多个声明身份: {declarations:?}")
            }
        }
    }
}

impl std::error::Error for SemanticClosureError {}

/// 从不可变源码输入计算 Compiler 内部选择。入口身份只由 `CompilerSourceGroup::is_entry`
/// 提供，不按依赖顺序或末项位置推断；装配层不能提交语义计划。
///
/// `arch` 是 TGrammar 模板展开的目标键：索引必须在与 HIR lowering 相同的展开结果上
/// 识别 `[export]` / `[main]`，否则模板分支内的入口声明不可见。
pub(crate) fn source_group_closure(groups: &[crate::CompilerSourceGroup], arch: &str) -> Result<SemanticClosure, SemanticClosureError> {
    source_group_closure_with_host_bindings(groups, &[], arch)
}

/// 与 [`source_group_closure`] 相同，但把 Resolver 选定的 host provider 绑定作为外部事实：
/// 被本次入口闭包引用的合同，其选定 provider 声明必须进入 body lowering。
///
/// 未被引用的合同不物化 provider：装配层的选择基于声明库存，不掌握入口可达性。
pub(crate) fn source_group_closure_with_host_bindings(
    groups: &[crate::CompilerSourceGroup],
    host_bindings: &[crate::valkyrie::compile_pipeline::CompilerHostProviderBinding],
    arch: &str,
) -> Result<SemanticClosure, SemanticClosureError> {
    let owned = groups
        .iter()
        .map(|group| group.sources.iter().map(|file| (file.path.clone(), file.source.clone())).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let dependency_keys =
        groups.iter().map(|group| group.direct_dependencies.iter().map(String::as_str).collect::<Vec<_>>()).collect::<Vec<_>>();
    let inputs = groups
        .iter()
        .zip(&owned)
        .zip(&dependency_keys)
        .map(|((group, files), dependencies)| ClosureGroupInput {
            dependency_key: group.dependency_key.as_str(),
            module_name: group.name.as_str(),
            direct_dependencies: dependencies.as_slice(),
            is_entry: group.is_entry,
            files: files.as_slice(),
        })
        .collect::<Vec<_>>();
    plan_semantic_closure_with_host_bindings(&inputs, host_bindings, arch)
}

/// 从入口组与其依赖 inventory 规划完整语义闭包；`arch` 决定 TGrammar 模板分支。
pub fn plan_semantic_closure(groups: &[ClosureGroupInput<'_>], arch: &str) -> Result<SemanticClosure, SemanticClosureError> {
    plan_semantic_closure_with_host_bindings(groups, &[], arch)
}

/// 从入口组、依赖 inventory 与 Resolver 的 host provider 选择规划完整语义闭包。
pub fn plan_semantic_closure_with_host_bindings(
    groups: &[ClosureGroupInput<'_>],
    host_bindings: &[crate::valkyrie::compile_pipeline::CompilerHostProviderBinding],
    arch: &str,
) -> Result<SemanticClosure, SemanticClosureError> {
    let inventory = groups.iter().map(|group| group.dependency_key).collect::<BTreeSet<_>>();
    // 可见性：本组 + 直接依赖组。这是闭包允许跨组命中声明的唯一范围；
    // 装配层未声明的依赖组不构成可见性，跨组全局名字回退一律不存在。
    let visibility = groups
        .iter()
        .map(|group| {
            let mut visible = BTreeSet::from([group.dependency_key.to_string()]);
            visible.extend(
                group
                    .direct_dependencies
                    .iter()
                    .filter(|dependency| inventory.contains(*dependency))
                    .map(|dependency| (*dependency).to_string()),
            );
            (group.dependency_key.to_string(), visible)
        })
        .collect::<BTreeMap<String, BTreeSet<String>>>();

    let mut indexed_files = BTreeMap::<(String, String), IndexedFile>::new();
    let mut declaration_index = BTreeMap::<DeclarationKey, DeclarationIndexEntry>::new();
    let mut indexes = DeclarationIndexes::default();
    let mut unindexed_source_files = BTreeMap::<String, String>::new();

    for group in groups.iter() {
        for (path, source) in group.files.iter() {
            match index_source_file(group.dependency_key, path, source, arch) {
                Ok(indexed) => {
                    for entry in &indexed.declarations {
                        indexes.register(entry.clone());
                        declaration_index.insert(entry.key.clone(), entry.clone());
                    }
                    for (variant, owner) in &indexed.variant_members {
                        indexes.register_variant(variant, owner);
                    }
                    indexed_files.insert((group.dependency_key.to_string(), path.clone()), indexed);
                }
                Err(reason) => {
                    unindexed_source_files.insert(format!("{}::{path}", group.dependency_key), reason);
                }
            }
        }
    }
    // 聚合身份与成员身份、别名与目标的关联必须在整个 inventory 索引完成后统一按声明身份建立：
    // 聚合与它的成员可以在同组的不同文件里，也可以跨依赖组（std 的 `imply i64: Hash` 与 core 的
    // `structure i64`），按文件边界或「短名 + namespace」连接都是名字近似。
    indexes.link(&visibility);

    let mut demanded = BTreeSet::new();
    let mut selectors = BTreeSet::<String>::new();
    let mut queue = VecDeque::new();
    let mut touched_files = BTreeSet::<(String, String)>::new();
    for group in groups.iter() {
        if !group.is_entry {
            continue;
        }
        // 入口组只为显式 `[export]` / `[main]` 声明提供 seed。
        // 「文件属于入口组」或「组顺序末项」都不是需求事实，不能提升组内全部声明。
        let mut seeded = false;
        for (path, _) in group.files.iter() {
            let file_id = (group.dependency_key.to_string(), path.clone());
            let Some(indexed) = indexed_files.get(&file_id)
            else {
                continue;
            };
            if !indexed.declarations.iter().any(|entry| entry.entry_point.is_some()) {
                continue;
            }
            for entry in &indexed.declarations {
                if entry.entry_point.is_some() {
                    queue.push_back(entry.key.clone());
                    seeded = true;
                }
            }
            // 入口声明所在文件的 `using`/顶层语句是程序的一部分，
            // 必须与声明引用一起展开，否则只含模块级语句的文件会缺依赖。
            if touched_files.insert(file_id) {
                push_scoped_references(indexed, group.dependency_key, &visibility, &indexes, &mut selectors, &mut queue);
            }
        }
        if !seeded {
            let prefix = format!("{}::", group.dependency_key);
            let unindexed_source_files = unindexed_source_files
                .range(prefix.clone()..)
                .take_while(|(file, _)| file.starts_with(&prefix))
                .map(|(file, reason)| (file[prefix.len()..].to_string(), reason.clone()))
                .collect();
            return Err(SemanticClosureError::MissingEntryDeclaration {
                dependency_key: group.dependency_key.to_string(),
                unindexed_source_files,
            });
        }
    }
    // Resolver 选定的 host provider 是需求 seed，与入口声明同等对待：必须先进入固定点，
    // 才能把 provider 引用的类型、成员与辅助函数纳入闭包。若在固定点之后才插入 demanded，
    // provider 的引用永远不被遍历，被引用的聚合不会进入依赖组 HIR，随后类型绑定必然失败。
    enqueue_selected_host_providers(host_bindings, &declaration_index, &mut queue)?;

    let empty_visibility = BTreeSet::new();
    loop {
        while let Some(key) = queue.pop_front() {
            if !demanded.insert(key.clone()) {
                continue;
            }
            let entry = match declaration_index.get(&key) {
                Some(entry) => entry.clone(),
                None => continue,
            };
            let visible = visibility.get(&key.dependency_key).unwrap_or(&empty_visibility);
            if touched_files.insert((key.dependency_key.clone(), key.source_path.clone())) {
                if let Some(file) = indexed_files.get(&(key.dependency_key.clone(), key.source_path.clone())) {
                    push_scoped_references(file, &key.dependency_key, &visibility, &indexes, &mut selectors, &mut queue);
                }
            }
            // 成员方法被需求时，其拥有者聚合的声明外壳同样必须进入闭包。
            if let Some(owner) = &key.owner {
                for resolved in indexes.resolve(&[owner.clone()], &key.namespace, visible) {
                    queue.push_back(resolved);
                }
            }
            selectors.extend(entry.selectors.iter().cloned());
            for reference in &entry.references {
                for resolved in indexes.resolve(reference, &key.namespace, visible) {
                    queue.push_back(resolved);
                }
            }
        }
        // 选择名驱动的成员提升：成员调用的接收者类型在索引阶段不可知，
        // 因此只有「该名字确实出现在闭包需求中」且「其拥有聚合已被需求」两条
        // 结构化事实同时成立时，成员才作为该聚合的成员进入闭包。
        let mut promoted = BTreeSet::new();
        for aggregate in &demanded {
            if !indexes.is_owner(aggregate) {
                continue;
            }
            for member in indexes.selected_members(aggregate, &selectors) {
                if !demanded.contains(&member) {
                    promoted.insert(member);
                }
            }
        }
        if promoted.is_empty() {
            break;
        }
        queue.extend(promoted);
    }

    let mut closure_groups = Vec::with_capacity(groups.len());
    for group in groups.iter() {
        let dependency_key = group.dependency_key;
        // body lowering 边界只由被需求声明身份产生；入口组不再按文件整体下沉。
        let mut body_lowering_source_paths = BTreeSet::new();
        let mut body_lowering_declarations = BTreeSet::new();
        for key in &demanded {
            if key.dependency_key == dependency_key {
                body_lowering_declarations.insert(key.clone());
                body_lowering_source_paths.insert(key.source_path.clone());
            }
        }
        closure_groups.push(GroupSemanticClosure {
            dependency_key: dependency_key.to_string(),
            body_lowering_source_paths,
            body_lowering_declarations,
        });
    }

    let digest = compute_closure_digest(&closure_groups, &indexed_files, arch);
    Ok(SemanticClosure { groups: closure_groups, declaration_index, demanded_declarations: demanded, unindexed_source_files, digest })
}

fn enqueue_selected_host_providers(
    host_bindings: &[crate::valkyrie::compile_pipeline::CompilerHostProviderBinding],
    declaration_index: &BTreeMap<DeclarationKey, DeclarationIndexEntry>,
    queue: &mut VecDeque<DeclarationKey>,
) -> Result<(), SemanticClosureError> {
    for binding in host_bindings {
        if !declaration_index.contains_key(&binding.provider) {
            return Err(SemanticClosureError::HostProviderNotResolved { symbol: binding.provider.encoded() });
        }
        queue.push_back(binding.provider.clone());
    }
    Ok(())
}

/// 构造顶层 `micro` 的冻结声明身份。
pub fn micro_declaration_key(dependency_key: &str, source_path: &str, namespace: &[String], name: &str) -> DeclarationKey {
    DeclarationKey {
        dependency_key: dependency_key.to_string(),
        source_path: source_path.to_string(),
        namespace: namespace.to_vec(),
        kind: DeclarationKind::Micro,
        owner: None,
        name: name.to_string(),
    }
}

/// 解析一组「namespace 段 + 名字路径」引用并把命中的声明身份入队；
/// 文件级成员选择名只登记为选择事实，不参与自由名解析。
fn push_scoped_references(
    file: &IndexedFile,
    group: &str,
    visibility: &BTreeMap<String, BTreeSet<String>>,
    indexes: &DeclarationIndexes,
    selectors: &mut BTreeSet<String>,
    queue: &mut VecDeque<DeclarationKey>,
) {
    let empty = BTreeSet::new();
    let visible = visibility.get(group).unwrap_or(&empty);
    selectors.extend(file.module_selectors.iter().cloned());
    for (namespace, reference) in &file.module_references {
        for resolved in indexes.resolve(reference, namespace, visible) {
            queue.push_back(resolved);
        }
    }
}

/// 声明索引：闭包规划消费的唯一名字事实来源。
///
/// 索引只登记声明身份与结构关系（依赖组、namespace、种类、拥有者），不复制 HIR 事实：
/// namespace 链与可见组回退都只是「目标模块能看见哪些声明」的上界判定，
/// 精确重载解析、类型匹配与歧义判定仍由 HIR 完成。
#[derive(Debug, Default)]
struct DeclarationIndexes {
    /// `(namespace, 短名) → 自由声明`；namespace 链解析优先使用它。成员声明不登记：
    /// 成员选择名不建立全库同名索引，只能由「拥有者聚合身份 + 选择名」产生。
    names: BTreeMap<(Vec<String>, String), BTreeSet<DeclarationKey>>,
    /// `短名 → 自由声明`；只用于可见组内的回退查询。
    by_name: BTreeMap<String, BTreeSet<DeclarationKey>>,
    /// 变体名 → 拥有它的 sum/unite 声明。
    variant_owners: BTreeMap<String, BTreeSet<DeclarationKey>>,
    /// 待连接的成员声明：`(依赖组, namespace, 拥有者引用, 成员身份)`。
    pending_members: Vec<PendingMember>,
    /// 待展开的类型别名目标：别名身份 → `(别名 namespace, 目标引用)`。
    alias_targets: BTreeMap<DeclarationKey, (Vec<String>, ReferencePath)>,
    /// 名义聚合声明 → 它的成员声明。
    members: BTreeMap<DeclarationKey, BTreeSet<DeclarationKey>>,
}

/// 一条尚未连接的成员声明：拥有者只以引用路径记录，连接时按声明身份解析。
#[derive(Debug)]
struct PendingMember {
    group: String,
    namespace: Vec<String>,
    owner_reference: ReferencePath,
    key: DeclarationKey,
}

impl DeclarationIndexes {
    /// 登记一条声明。
    ///
    /// 成员声明（`method` / `imply_method`）只进入待连接的拥有者关联，不进入任何短名索引；
    /// 类型别名记录目标引用，供成员拥有者解析时穿透别名取到真正的名义聚合。
    fn register(&mut self, entry: DeclarationIndexEntry) {
        let DeclarationIndexEntry { key, link, .. } = entry;
        if key.kind.is_member() {
            // 拥有者引用不可解析（例如 imply 目标不是名字路径）时 fail-closed：
            // 该成员既不会被裸名命中，也不会被提升，只作为不可达声明存在。
            if let Some(DeclarationLink::MemberOwner(owner_reference)) = link {
                self.pending_members.push(PendingMember {
                    group: key.dependency_key.clone(),
                    namespace: key.namespace.clone(),
                    owner_reference,
                    key,
                });
            }
            return;
        }
        if let Some(DeclarationLink::AliasTarget(target)) = link {
            self.alias_targets.insert(key.clone(), (key.namespace.clone(), target));
        }
        self.names.entry((key.namespace.clone(), key.name.clone())).or_default().insert(key.clone());
        self.by_name.entry(key.name.clone()).or_default().insert(key);
    }

    /// 登记 unity 变体名与它的拥有者 sum/unite 声明。
    fn register_variant(&mut self, variant: &str, owner: &DeclarationKey) {
        self.variant_owners.entry(variant.to_string()).or_default().insert(owner.clone());
    }

    /// 把成员声明挂到其拥有者聚合身份上。
    ///
    /// 连接发生在整个 inventory 索引完成后：拥有者与成员可以在同组的不同文件里，也可以在不同的
    /// 依赖组里。拥有者引用按成员所属组的可见性解析，因此未声明的依赖组不会成为拥有者；
    /// 解析不到拥有者时成员关联不成立，也不会被名字回退补上。
    fn link(&mut self, visibility: &BTreeMap<String, BTreeSet<String>>) {
        let empty = BTreeSet::new();
        for member in std::mem::take(&mut self.pending_members) {
            let visible = visibility.get(&member.group).unwrap_or(&empty);
            for owner in self.resolve_aggregate(&member.owner_reference, &member.namespace, visible) {
                self.members.entry(owner).or_default().insert(member.key.clone());
            }
        }
    }

    /// 解析一个「必须指向名义聚合」的引用：类型别名不是聚合身份，穿透其目标继续解析。
    fn resolve_aggregate(&self, reference: &[String], namespace: &[String], visible: &BTreeSet<String>) -> BTreeSet<DeclarationKey> {
        let mut aggregates = BTreeSet::new();
        let mut frontier = vec![(reference.to_vec(), namespace.to_vec())];
        let mut visited = BTreeSet::new();
        while let Some((reference, namespace)) = frontier.pop() {
            if !visited.insert((reference.clone(), namespace.clone())) {
                continue;
            }
            for key in self.resolve(&reference, &namespace, visible) {
                match self.alias_targets.get(&key) {
                    Some((alias_namespace, target)) => frontier.push((target.clone(), alias_namespace.clone())),
                    None => {
                        aggregates.insert(key);
                    }
                }
            }
        }
        aggregates
    }

    /// 该声明身份是否为携带成员的名义聚合。
    fn is_owner(&self, key: &DeclarationKey) -> bool {
        self.members.contains_key(key)
    }

    /// 该聚合上「确实出现在闭包需求中」的成员：选择名是需求事实，拥有者是声明身份，
    /// 两者都成立才提升，不做全库同名成员回退。
    fn selected_members(&self, aggregate: &DeclarationKey, selectors: &BTreeSet<String>) -> BTreeSet<DeclarationKey> {
        self.members
            .get(aggregate)
            .map(|members| members.iter().filter(|member| selectors.contains(&member.name)).cloned().collect())
            .unwrap_or_default()
    }

    /// 只保留可见组内的声明：可见性是跨组命中的唯一闸门，本组之外的未声明依赖不得命中。
    fn retain_visible(&self, keys: &BTreeSet<DeclarationKey>, visible: &BTreeSet<String>) -> BTreeSet<DeclarationKey> {
        keys.iter().filter(|key| visible.contains(&key.dependency_key)).cloned().collect()
    }

    /// 解析单个名字路径引用，返回命中的声明身份集合。
    ///
    /// 命中规则与 `path-resolution.md` 一致：限定路径先按绝对路径，再按当前 namespace 逐级前缀；
    /// 单段名字沿 namespace 链向外查找。**每一步都受可见性约束**：只接受引用方本组与
    /// 直接依赖组的声明，未声明的依赖组即使名字命中也不进入闭包。
    ///
    /// - 限定路径未命中 namespace 时，前缀按同一规则解析为名义聚合身份，再取该聚合的成员身份；
    /// - 单段名字在 namespace 链未命中时，只在可见组内按短名回退，且**只接受非成员声明**：
    ///   成员调用按接收者类型解析的事实由「聚合身份 + 已出现的选择名」提供；
    /// - 都不是时只允许变体名命中其拥有者 sum/unite 声明这一条结构化路径。
    fn resolve(&self, reference: &[String], namespace: &[String], visible: &BTreeSet<String>) -> BTreeSet<DeclarationKey> {
        let name = match reference.last() {
            Some(name) => name,
            None => return BTreeSet::new(),
        };
        if reference.len() > 1 {
            let prefix = &reference[..reference.len() - 1];
            if let Some(keys) = self.names.get(&(prefix.to_vec(), name.clone())) {
                let resolved = self.retain_visible(keys, visible);
                if !resolved.is_empty() {
                    return resolved;
                }
            }
            for depth in (0..=namespace.len()).rev() {
                let mut scoped = namespace[..depth].to_vec();
                scoped.extend_from_slice(prefix);
                if let Some(keys) = self.names.get(&(scoped, name.clone())) {
                    let resolved = self.retain_visible(keys, visible);
                    if !resolved.is_empty() {
                        return resolved;
                    }
                }
            }
            // `<聚合>.<成员>`：解析聚合身份后按拥有者身份取成员，不按名字认亲。
            let mut members = BTreeSet::new();
            for owner in self.resolve(prefix, namespace, visible) {
                if let Some(keys) = self.members.get(&owner) {
                    members.extend(keys.iter().filter(|key| key.name == *name).cloned());
                }
            }
            return members;
        }
        // 单段名字沿 namespace 链向外查找，链上未命中时只在可见组内按短名回退。
        // 这里不区分「内建」与用户声明：`usize` / `utf8` 等同样是由源码声明、拥有
        // `[primitive]` 注解的普通类型身份，按名字表放行或拒绝都属于第二份声明事实。
        for depth in (0..=namespace.len()).rev() {
            let scoped = namespace[..depth].to_vec();
            if let Some(keys) = self.names.get(&(scoped, name.clone())) {
                let resolved = self.retain_visible(keys, visible);
                if !resolved.is_empty() {
                    return resolved;
                }
            }
        }
        if let Some(keys) = self.by_name.get(name) {
            let resolved = self.retain_visible(keys, visible);
            if !resolved.is_empty() {
                return resolved;
            }
        }
        self.variant_owners.get(name).map(|keys| self.retain_visible(keys, visible)).unwrap_or_default()
    }
}

fn compute_closure_digest(groups: &[GroupSemanticClosure], files: &BTreeMap<(String, String), IndexedFile>, arch: &str) -> String {
    let mut parts = vec![format!("planner:{CLOSURE_PLANNER_VERSION}"), format!("arch:{arch}")];
    for group in groups {
        parts.push(format!("group:{}", group.dependency_key));
        for key in &group.body_lowering_declarations {
            parts.push(format!("decl:{}", key.encoded()));
        }
        for path in &group.body_lowering_source_paths {
            parts.push(format!("path:{path}"));
            if let Some(file) = files.get(&(group.dependency_key.clone(), path.clone())) {
                parts.push(format!("content:{}", file.source_digest));
            }
        }
    }
    let borrowed = parts.iter().map(String::as_str).collect::<Vec<_>>();
    combined_content_hash(&borrowed)
}

struct IndexedFile {
    source_digest: String,
    declarations: Vec<DeclarationIndexEntry>,
    /// 变体名 → 拥有该变体的 sum/unite 声明。
    variant_members: Vec<(String, DeclarationKey)>,
    /// 文件级语句（`using`、顶层 `let`/表达式）的自由名引用。
    module_references: BTreeSet<ScopedReference>,
    /// 文件级语句的成员选择名；与声明级选择名一样只参与成员提升。
    module_selectors: BTreeSet<String>,
}

fn index_source_file(dependency_key: &str, path: &str, source: &str, arch: &str) -> Result<IndexedFile, String> {
    let mut root = parse_source(source).map_err(|error| error.to_string())?;
    // TGrammar 模板分支必须按编译架构展开后再建索引：入口声明可能只在某个分支里，
    // 直接索引模板节点会让 `[export]` / `[main]` 在索引阶段不可见。
    crate::valkyrie::hir::lowering::template_expand::expand_templates_in_root(&mut root, source, arch).map_err(|error| error.to_string())?;
    let mut builder = FileBuilder::new(dependency_key, path);
    collect_items(&mut builder, &[], &root.items);
    Ok(IndexedFile {
        source_digest: combined_content_hash(&[source]),
        declarations: builder.declarations,
        variant_members: builder.variant_members,
        module_references: builder.module_references,
        module_selectors: builder.module_selectors,
    })
}

/// 单个文件索引过程中收集到的声明与文件级引用。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FileBuilder {
    dependency_key: String,
    source_path: String,
    declarations: Vec<DeclarationIndexEntry>,
    variant_members: Vec<(String, DeclarationKey)>,
    module_references: BTreeSet<ScopedReference>,
    module_selectors: BTreeSet<String>,
    import_paths: BTreeSet<String>,
}

impl FileBuilder {
    fn new(dependency_key: &str, source_path: &str) -> Self {
        Self { dependency_key: dependency_key.to_string(), source_path: source_path.to_string(), ..Self::default() }
    }

    fn register(
        &mut self,
        namespace: &[String],
        kind: DeclarationKind,
        name: &str,
        owner: Option<&str>,
        link: Option<DeclarationLink>,
        entry_point: Option<EntryPointKind>,
        references: References,
    ) -> DeclarationKey {
        let key = DeclarationKey {
            dependency_key: self.dependency_key.clone(),
            source_path: self.source_path.clone(),
            namespace: namespace.to_vec(),
            kind,
            owner: owner.map(str::to_string),
            name: name.to_string(),
        };
        self.declarations.push(DeclarationIndexEntry {
            key: key.clone(),
            link,
            entry_point,
            references: references.paths,
            selectors: references.selectors,
            import_paths: references.imports,
        });
        key
    }

    fn register_module_references(&mut self, namespace: &[String], references: References) {
        for path in references.paths {
            self.module_references.insert((namespace.to_vec(), path));
        }
        self.module_selectors.extend(references.selectors);
        self.import_paths.extend(references.imports);
    }
}

/// 单个声明（或文件级语句）引用的自由名路径、成员选择名与 import 路径。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct References {
    paths: BTreeSet<ReferencePath>,
    /// 成员选择名；只能与拥有聚合身份结合参与成员提升，不参与自由名解析。
    selectors: BTreeSet<String>,
    imports: BTreeSet<String>,
}

fn name_path_segments(path: &ast::NamePath) -> Vec<String> {
    path.parts.iter().map(|part| part.name.clone()).collect()
}

/// 按被需求声明身份裁剪 AST：非被需求声明不得进入 HIR 物化。
///
/// 与 `collect_item` 共用同一套「AST 条目 → [`DeclarationKey`]」映射；声明级物化边界
/// 必须与闭包规划使用同一身份，不得用文件名或短名近似。文件级语句（`using`、
/// 顶层 `let`/表达式）与无 body 的 `namespace` 条目不是声明，保留。
pub fn retain_demanded_declarations(
    items: &mut Vec<ast::StatementNode>,
    dependency_key: &str,
    source_path: &str,
    demanded: &BTreeSet<DeclarationKey>,
) {
    let key_of = |namespace: &[String], kind: DeclarationKind, name: &str, owner: Option<&str>| DeclarationKey {
        dependency_key: dependency_key.to_string(),
        source_path: source_path.to_string(),
        namespace: namespace.to_vec(),
        kind,
        owner: owner.map(str::to_string),
        name: name.to_string(),
    };
    retain_items(items, &[], demanded, &key_of);
}

fn retain_items(
    items: &mut Vec<ast::StatementNode>,
    namespace: &[String],
    demanded: &BTreeSet<DeclarationKey>,
    key_of: &impl Fn(&[String], DeclarationKind, &str, Option<&str>) -> DeclarationKey,
) {
    // 与 `collect_items` 保持同一套 namespace 语义：分号式 `namespace X;` 切换后续条目的
    // namespace，因此这里必须顺序推进作用域，不能对每个条目都用同一个起始 namespace，
    // 否则分号式 namespace 下的声明会按空 namespace 查身份并被误裁掉。
    let mut current = namespace.to_vec();
    items.retain_mut(|item| {
        if let ast::StatementNode::Namespace(declaration) = item {
            if !declaration.has_body() {
                current = name_path_segments(&declaration.name);
                return true;
            }
        }
        keep_declaration(item, &current, demanded, key_of)
    });
}

/// 条目保留规则：声明条目按身份保留；作用域与语句条目保留并递归裁剪其声明子项。
fn keep_declaration(
    item: &mut ast::StatementNode,
    namespace: &[String],
    demanded: &BTreeSet<DeclarationKey>,
    key_of: &impl Fn(&[String], DeclarationKind, &str, Option<&str>) -> DeclarationKey,
) -> bool {
    match item {
        // `namespace` 是作用域语句：带 body 时其条目以声明路径为 namespace 递归裁剪。
        ast::StatementNode::Namespace(decl) => {
            if let Some(body) = &mut decl.body {
                let declared = name_path_segments(&decl.name);
                retain_items(&mut body.items, &declared, demanded, key_of);
            }
            true
        }
        ast::StatementNode::Class(class) => {
            retain_members(&mut class.body.methods, namespace, &class.name.name, demanded, key_of);
            demanded.contains(&key_of(namespace, DeclarationKind::Class, &class.name.name, None)) || !class.body.methods.is_empty()
        }
        ast::StatementNode::Structure(structure) => {
            retain_members(&mut structure.body.methods, namespace, &structure.name.name, demanded, key_of);
            demanded.contains(&key_of(namespace, DeclarationKind::Structure, &structure.name.name, None)) || !structure.body.methods.is_empty()
        }
        ast::StatementNode::Trait(trait_decl) => {
            retain_members(&mut trait_decl.body.methods, namespace, &trait_decl.name.name, demanded, key_of);
            demanded.contains(&key_of(namespace, DeclarationKind::Trait, &trait_decl.name.name, None)) || !trait_decl.body.methods.is_empty()
        }
        ast::StatementNode::Union(union_def) => demanded.contains(&key_of(namespace, DeclarationKind::Union, &union_def.name.name, None)),
        ast::StatementNode::Flags(flags) => demanded.contains(&key_of(namespace, DeclarationKind::Flags, &flags.name.name, None)),
        ast::StatementNode::Enums(enums) => demanded.contains(&key_of(namespace, DeclarationKind::Enum, &enums.name.name, None)),
        ast::StatementNode::Micro(micro) => demanded.contains(&key_of(namespace, DeclarationKind::Micro, &micro.name.name, None)),
        ast::StatementNode::TypeAlias(alias) => demanded.contains(&key_of(namespace, DeclarationKind::TypeAlias, &alias.name.name, None)),
        ast::StatementNode::TypeFunction(type_function) => {
            demanded.contains(&key_of(namespace, DeclarationKind::TypeFunction, &type_function.name.name, None))
        }
        // imply 块的每个 witness 方法各自是一份被需求声明；拥有者与成员名都与 `collect_imply` 一致。
        ast::StatementNode::Imply(imply) => {
            let owner = imply_owner_name(imply);
            imply.methods.retain(|method| {
                let name = operator_members::declared_member_name(&method.name.name, &method.annotations);
                demanded.contains(&key_of(namespace, DeclarationKind::ImplyMethod, &name, owner.as_deref()))
            });
            !imply.methods.is_empty()
        }
        // 包装语句按其内部条目的身份保留或丢弃，包装节点本身不是声明。
        ast::StatementNode::Statement(inner) => keep_declaration(inner.as_mut(), namespace, demanded, key_of),
        // 文件级 `using` / `let` / 顶层表达式不是声明；`singleton` / `widget` / `property`
        // 目前不在声明索引里，只能整条保留（缺口见账本）。
        _ => true,
    }
}

fn retain_members(
    methods: &mut Vec<ast::MethodDeclaration>,
    namespace: &[String],
    owner: &str,
    demanded: &BTreeSet<DeclarationKey>,
    key_of: &impl Fn(&[String], DeclarationKind, &str, Option<&str>) -> DeclarationKey,
) {
    methods.retain(|method| {
        let name = operator_members::declared_member_name(&method.name.name, &method.annotations);
        demanded.contains(&key_of(namespace, DeclarationKind::Method, &name, Some(owner)))
    });
}

fn collect_items(builder: &mut FileBuilder, current_namespace: &[String], items: &[ast::StatementNode]) {
    let mut namespace = current_namespace.to_vec();
    for item in items {
        match item {
            ast::StatementNode::Namespace(namespace_decl) => {
                let declared = name_path_segments(&namespace_decl.name);
                if !namespace_decl.has_body() {
                    namespace = declared;
                }
                else if let Some(body) = &namespace_decl.body {
                    // 带 body 的 `namespace` 以自身路径作为条目 namespace（与 HIR lowering 一致）。
                    collect_items(builder, &declared, &body.items);
                }
            }
            other => collect_item(builder, &namespace, other),
        }
    }
}

fn collect_item(builder: &mut FileBuilder, namespace: &[String], item: &ast::StatementNode) {
    match item {
        ast::StatementNode::Using(using) => {
            let mut references = References::default();
            collect_using(using, &mut references);
            builder.register_module_references(namespace, references);
        }
        ast::StatementNode::Class(class) => {
            let mut signature = References::default();
            collect_signature_fields(&class.body.fields, &mut signature);
            builder.register(namespace, DeclarationKind::Class, &class.name.name, None, None, None, signature);
            collect_members(builder, namespace, &class.name.name, &class.body.methods);
        }
        ast::StatementNode::Structure(structure) => {
            let mut signature = References::default();
            collect_signature_fields(&structure.body.fields, &mut signature);
            builder.register(namespace, DeclarationKind::Structure, &structure.name.name, None, None, None, signature);
            collect_members(builder, namespace, &structure.name.name, &structure.body.methods);
        }
        ast::StatementNode::Trait(trait_decl) => {
            let mut signature = References::default();
            collect_signature_fields(&trait_decl.body.fields, &mut signature);
            builder.register(namespace, DeclarationKind::Trait, &trait_decl.name.name, None, None, None, signature);
            collect_members(builder, namespace, &trait_decl.name.name, &trait_decl.body.methods);
        }
        ast::StatementNode::Union(union_def) => {
            let mut signature = References::default();
            collect_signature_fields(&union_def.body.fields, &mut signature);
            builder.register(namespace, DeclarationKind::Union, &union_def.name.name, None, None, None, signature);
        }
        ast::StatementNode::Flags(flags) => {
            builder.register(namespace, DeclarationKind::Flags, &flags.name.name, None, None, None, References::default());
        }
        ast::StatementNode::Enums(enums) => {
            let key = builder.register(namespace, DeclarationKind::Enum, &enums.name.name, None, None, None, References::default());
            for variant in &enums.variants {
                builder.variant_members.push((variant.name.name.clone(), key.clone()));
            }
        }
        ast::StatementNode::Imply(imply) => collect_imply(builder, namespace, imply),
        ast::StatementNode::Micro(micro) => {
            let mut references = References::default();
            collect_micro(micro, &mut references);
            let entry_point = entry_point_annotation(&micro.annotations);
            builder.register(namespace, DeclarationKind::Micro, &micro.name.name, None, None, entry_point, references);
        }
        ast::StatementNode::TypeAlias(alias) => {
            let mut references = References::default();
            collect_type_expression(&alias.target, &mut references);
            let link = type_target_reference(&alias.target).map(DeclarationLink::AliasTarget);
            builder.register(namespace, DeclarationKind::TypeAlias, &alias.name.name, None, link, None, references);
        }
        ast::StatementNode::TypeFunction(type_function) => {
            let mut references = References::default();
            collect_type_function(type_function, &mut references);
            builder.register(namespace, DeclarationKind::TypeFunction, &type_function.name.name, None, None, None, references);
        }
        ast::StatementNode::Statement(inner) => collect_item(builder, namespace, inner),
        ast::StatementNode::Let(let_stmt) => {
            let mut references = References::default();
            collect_let(let_stmt, &mut references);
            builder.register_module_references(namespace, references);
        }
        ast::StatementNode::ExprStmt(expr_stmt) => {
            let mut references = References::default();
            collect_term(&expr_stmt.expr, &mut references);
            builder.register_module_references(namespace, references);
        }
        _ => {}
    }
}

fn collect_members(builder: &mut FileBuilder, namespace: &[String], owner: &str, methods: &[ast::MethodDeclaration]) {
    for method in methods {
        let mut references = References::default();
        collect_method(method, &mut references);
        // 成员身份使用与 HIR 相同的运算符成员名：`[infix] micro \`+\`` 的身份是 `infix +`，
        // 否则调用点（`a + b`）的选择名永远命不中声明。
        let name = operator_members::declared_member_name(&method.name.name, &method.annotations);
        // 拥有者引用相对成员自身 namespace：聚合与该成员同文件同 namespace 时按 namespace 链命中。
        let link = Some(DeclarationLink::MemberOwner(vec![owner.to_string()]));
        builder.register(namespace, DeclarationKind::Method, &name, Some(owner), link, None, references);
    }
}

fn collect_imply(builder: &mut FileBuilder, namespace: &[String], imply: &ast::ImplyDeclaration) {
    let mut signature = References::default();
    collect_type_expression(&imply.target_type, &mut signature);
    if let Some(trait_type) = &imply.trait_type {
        collect_type_expression(trait_type, &mut signature);
    }
    if imply.methods.is_empty() {
        // 没有 witness 方法的 imply 只有外壳，作为文件级引用保存。
        builder.register_module_references(namespace, signature);
        return;
    }
    // witness 方法挂在被扩展聚合的**声明身份**上：拥有者是 imply 目标类型的引用路径，
    // 由索引在整个 inventory 完成后按可见性解析（可以跨依赖组），不按短名认亲。
    let owner = imply_owner_name(imply);
    let owner_reference = type_target_reference(&imply.target_type);
    for method in &imply.methods {
        // imply 的目标类型与 trait 引用附在每个 witness 方法上：需求任一 witness 都需要它们可解析。
        let mut references = signature.clone();
        collect_method(method, &mut references);
        let name = operator_members::declared_member_name(&method.name.name, &method.annotations);
        let link = owner_reference.clone().map(DeclarationLink::MemberOwner);
        builder.register(namespace, DeclarationKind::ImplyMethod, &name, owner.as_deref(), link, None, references);
    }
}

/// imply 目标类型的聚合短名：`imply HashMap<K, V>` / `imply std::collection::HashMap<K, V>` → `HashMap`。
fn imply_owner_name(imply: &ast::ImplyDeclaration) -> Option<String> {
    type_target_reference(&imply.target_type).and_then(|segments| segments.last().cloned())
}

/// 类型表达式被引用的名字路径：`Utf8Text` → `["Utf8Text"]`，`std::collection::HashMap<K, V>` → `["std","collection","HashMap"]`。
///
/// 只剥掉泛型应用取名字路径，不解释类型本身：声明身份由索引按整个 inventory 与可见性解析。
fn type_target_reference(ty: &ast::TypeExpression) -> Option<ReferencePath> {
    match ty {
        ast::TypeExpression::Namepath(path) => Some(name_path_segments(path)),
        ast::TypeExpression::Apply(apply) => type_target_reference(&apply.base),
        _ => None,
    }
}

fn collect_using(using: &ast::UsingDeclaration, references: &mut References) {
    references.imports.insert(using.path.parts.iter().map(|part| part.name.as_str()).collect::<Vec<_>>().join("::"));
    references.paths.insert(name_path_segments(&using.path));
    for import in &using.imports {
        references.paths.insert(vec![import.name.clone()]);
    }
    if let Some(alias) = &using.alias {
        references.paths.insert(vec![alias.name.clone()]);
    }
}

fn collect_signature_fields(fields: &[ast::FieldDeclaration], references: &mut References) {
    for field in fields {
        collect_type_expression(&field.typing, references);
    }
}

/// 声明的显式入口注解：`[export]` 优先于 `[main]`；二者都只提供 seed，不改变声明身份。
fn entry_point_annotation(annotations: &[ast::Attribute]) -> Option<EntryPointKind> {
    let mut entry_point = None;
    for attribute in annotations {
        match attribute.name.name.as_str() {
            "export" => return Some(EntryPointKind::Export),
            "main" => entry_point = Some(EntryPointKind::Main),
            _ => {}
        }
    }
    entry_point
}

fn collect_micro(micro: &ast::MicroDeclaration, references: &mut References) {
    for param in &micro.params {
        if let Some(ty) = &param.ty {
            collect_type_expression(ty, references);
        }
    }
    if let Some(return_type) = &micro.return_type {
        collect_type_expression(return_type, references);
    }
    collect_where_constraints(&micro.where_constraints, references);
    collect_block(&micro.body, references);
}

fn collect_method(method: &ast::MethodDeclaration, references: &mut References) {
    for param in &method.parameters {
        if let Some(ty) = &param.ty {
            collect_type_expression(ty, references);
        }
    }
    if let Some(return_type) = &method.return_type {
        collect_type_expression(return_type, references);
    }
    collect_where_constraints(&method.where_constraints, references);
    if let Some(body) = &method.body {
        collect_block(body, references);
    }
}

fn collect_type_function(type_function: &ast::TypeFunction, references: &mut References) {
    for param in &type_function.params {
        if let Some(ty) = &param.ty {
            collect_type_expression(ty, references);
        }
    }
    if let Some(return_type) = &type_function.return_type {
        collect_type_expression(return_type, references);
    }
    collect_where_constraints(&type_function.where_constraints, references);
    collect_block(&type_function.body, references);
}

/// `where` 约束的受约束类型、trait 路径、类型实参与关联等式右侧都要进入闭包：
/// 它们会被 HIR 冻结成 trait declaration identity，缺一就会出现「trait 约束未解析」。
fn collect_where_constraints(constraints: &[ast::type_nodes::WhereConstraint], references: &mut References) {
    for constraint in constraints {
        collect_type_expression(&constraint.target, references);
        for bound in &constraint.bounds {
            collect_name_path(&bound.trait_path, references);
            for argument in &bound.type_arguments {
                collect_type_expression(argument, references);
            }
            for equation in &bound.associated_types {
                collect_type_expression(&equation.ty, references);
            }
        }
    }
}

fn collect_let(let_stmt: &ast::Let, references: &mut References) {
    if let Some(ty) = &let_stmt.ty {
        collect_type_expression(ty, references);
    }
    if let Some(expr) = &let_stmt.expr {
        collect_term(expr, references);
    }
}

fn collect_block(block: &ast::Block, references: &mut References) {
    for statement in &block.statements {
        collect_statement(statement, references);
    }
}

fn collect_statement(statement: &ast::Statement, references: &mut References) {
    match statement {
        ast::Statement::Let(let_stmt) => collect_let(let_stmt, references),
        ast::Statement::Expression(expr_stmt) => collect_term(&expr_stmt.expr, references),
        _ => {}
    }
}

fn collect_term(term: &ast::TermExpression, references: &mut References) {
    match term {
        ast::TermExpression::ArrayLiteral { items, .. } | ast::TermExpression::TupleLiteral { items, .. } => {
            for item in items {
                collect_term(item, references);
            }
        }
        ast::TermExpression::Unary(node) => {
            // 运算符是普通成员调用：`!a` 降低为 `a.prefix !()`，`*a` 降低为 `a.deref_read()`。
            // 成员名与 HIR lowering 共用同一份映射，闭包据此把成员选择名登记进需求。
            if matches!(node.operator, ValkyrieTokenType::Star) {
                references.selectors.insert(operator_members::DEREF_READ_MEMBER.to_string());
            }
            else {
                // 负整数字面量在 HIR 折叠为字面量，不产生成员调用；这里仍登记该成员名，
                // 闭包只做上界判定：究竟走哪一个由 HIR 的降低规则决定。
                references.selectors.insert(operator_members::prefix_member_name(&node.operator));
            }
            collect_term(&node.base, references);
        }
        ast::TermExpression::Binary(node) => {
            collect_term(&node.lhs, references);
            collect_term(&node.rhs, references);
            match node.operator {
                // 赋值不是成员调用；对下标左值赋值降低为 `suffix []=` / `suffix ⁅⁆=`。
                ValkyrieTokenType::Eq => match &node.lhs {
                    ast::TermExpression::Index { .. } => {
                        references.selectors.insert(operator_members::SUBSCRIPT_ASSIGN_MEMBER.to_string());
                    }
                    ast::TermExpression::Offset { .. } => {
                        references.selectors.insert(operator_members::CARDINAL_SUBSCRIPT_ASSIGN_MEMBER.to_string());
                    }
                    _ => {}
                },
                // `&&` / `||` 降低为短路控制流，不产生成员调用。
                ValkyrieTokenType::AndAnd | ValkyrieTokenType::OrOr => {}
                _ => {
                    references.selectors.insert(operator_members::infix_member_name(&node.operator));
                }
            }
        }
        ast::TermExpression::Cast { expr, ty, .. } => {
            collect_term(expr, references);
            collect_type_expression(ty, references);
        }
        ast::TermExpression::Turbofish { expr, arguments, .. } => {
            collect_term(expr, references);
            for argument in arguments {
                collect_type_expression(argument, references);
            }
        }
        ast::TermExpression::ApplyCall { callee, args, .. } => {
            collect_call_target(callee, references);
            for arg in args {
                collect_term(arg, references);
            }
        }
        ast::TermExpression::DotCall { receiver, field, .. } => {
            collect_term(receiver, references);
            references.selectors.insert(field.name.clone());
        }
        ast::TermExpression::Index { receiver, index, .. } => {
            references.selectors.insert(operator_members::SUBSCRIPT_MEMBER.to_string());
            collect_term(receiver, references);
            collect_term(index, references);
        }
        ast::TermExpression::Offset { receiver, offset, .. } => {
            references.selectors.insert(operator_members::CARDINAL_SUBSCRIPT_MEMBER.to_string());
            collect_term(receiver, references);
            collect_term(offset, references);
        }
        ast::TermExpression::Group { expr, .. } => collect_term(expr, references),
        ast::TermExpression::Block(block) => collect_block(block, references),
        ast::TermExpression::Micro(micro) => collect_block(&micro.body, references),
        ast::TermExpression::Object { callee, fields, .. } => {
            collect_term(callee, references);
            for (_, value) in fields {
                if let Some(value) = value {
                    collect_term(value, references);
                }
            }
        }
        ast::TermExpression::If { condition, then_branch, else_branch, .. } => {
            collect_term(condition, references);
            collect_block(then_branch, references);
            if let Some(else_branch) = else_branch {
                collect_block(else_branch, references);
            }
        }
        ast::TermExpression::Match { scrutinee, arms, .. } => {
            collect_term(scrutinee, references);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    collect_term(guard, references);
                }
                collect_term(&arm.body, references);
            }
        }
        ast::TermExpression::NamePath(path) => collect_name_path(path, references),
        ast::TermExpression::Return(node) => {
            if let Some(expr) = &node.base {
                collect_term(expr, references);
            }
        }
        ast::TermExpression::Break(node) => {
            if let Some(expr) = &node.base {
                collect_term(expr, references);
            }
        }
        ast::TermExpression::Continue(_) => {}
        ast::TermExpression::Raise(node) => {
            if let Some(expr) = &node.base {
                collect_term(expr, references);
            }
        }
        ast::TermExpression::Resume(node) => {
            if let Some(expr) = &node.base {
                collect_term(expr, references);
            }
        }
        ast::TermExpression::Loop { condition, body, .. } => {
            if let Some(condition) = condition {
                collect_term(condition, references);
            }
            collect_block(body, references);
        }
        _ => {}
    }
}

fn collect_call_target(callee: &ast::TermExpression, references: &mut References) {
    match callee {
        ast::TermExpression::NamePath(path) => collect_name_path(path, references),
        ast::TermExpression::DotCall { receiver, field, .. } => {
            collect_term(receiver, references);
            references.selectors.insert(field.name.clone());
        }
        _ => collect_term(callee, references),
    }
}

fn collect_type_expression(ty: &ast::TypeExpression, references: &mut References) {
    match ty {
        ast::TypeExpression::Binary(node) => {
            collect_type_expression(&node.lhs, references);
            collect_type_expression(&node.rhs, references);
        }
        ast::TypeExpression::Unary(node) => collect_type_expression(&node.base, references),
        ast::TypeExpression::Generic(generic) => {
            references.paths.insert(vec![generic.name.name.clone()]);
        }
        ast::TypeExpression::Tuple(tuple) => {
            for element in &tuple.elements {
                collect_type_expression(&element.ty, references);
            }
        }
        ast::TypeExpression::Function(function) => {
            for param in &function.params {
                collect_type_expression(param, references);
            }
            collect_type_expression(&function.return_type, references);
        }
        ast::TypeExpression::Optional(optional) => collect_type_expression(&optional.inner, references),
        ast::TypeExpression::AssociatedType(associated) => {
            references.paths.insert(vec![associated.base.name.clone()]);
            references.paths.insert(vec![associated.name.name.clone()]);
        }
        ast::TypeExpression::QualifiedAssociatedType(qualified) => {
            collect_type_expression(&qualified.ty, references);
            collect_name_path(&qualified.trait_path, references);
            references.paths.insert(vec![qualified.name.name.clone()]);
        }
        ast::TypeExpression::Namepath(path) => collect_name_path(path, references),
        ast::TypeExpression::Apply(apply) => {
            collect_type_expression(&apply.base, references);
            for argument in &apply.arguments {
                collect_type_expression(argument, references);
            }
        }
    }
}

fn collect_name_path(path: &ast::NamePath, references: &mut References) {
    let segments = name_path_segments(path);
    if segments.is_empty() {
        return;
    }
    // 源码提到的每个名字都登记为引用：名字事实只有一份，就是声明索引。
    // `usize` / `utf8` 等标量类型同样由源码声明（`structure usize` / `type utf8`），
    // 不是「无声明内建」，因此不存在按名字表跳过的分支；未命中的引用自然解析为空。
    references.paths.insert(segments);
}
