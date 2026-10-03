# mir ssa

这里是 `MIR (SSA)` 的更细分目录。

## 职责
- 存放 `SSA` 专属结构和算法。
- 保持块参数、值引用、控制流约定集中定义。

## 禁止
- 不在这里混入目标相关 BackendPrivatePlan 或 runtime 逻辑。
- 不让 `ssa` 子目录重新长成第二份编译总入口。

## 调用事实所有权

调用合同必须从已解析声明传入，而不是由 MIR 或后端重新解析：

`源码闭包 → 声明解析与实例化 → 已类型化 HIR → Semantic MIR → 校验 → Canonical → 表示规划 → 目标私有计划`

- 声明解析拥有 callable declaration、owner、泛型 substitution 和完整签名。
- 重载选择必须保留所选声明及其实例身份；候选名称和实例化签名不能替代声明身份。
- 静态与实例方法仅由声明的 `self` 参数决定；没有 `self` 就是静态方法。
- Semantic MIR 的普通调用、operator 和集合方法统一引用已解析实例，不截断路径、不猜 owner、不补 receiver。
- 函数值调用引用 SSA 值及其已确定的函数类型，不伪装成具名静态函数。
- 依赖闭包只沿实例身份遍历；缺声明、类型代入、函数体或正式 import 合同时明确失败。
- Canonical、分区和目标私有计划消费同一身份；公开 ABI 名称只在互操作边界使用。
- 目标能力未实现时拒绝，不恢复 Symbol 分派、缺类型默认值或宿主代编译。

## 当前断链证据与实施边界

以下是当前源码的未完成项，不是允许保留的兼容合同：

1. `hir/overload.rs` 的候选携带 owner，但 `ResolvedOverload` 未保留该字段；
   `HirResolvedCall` 仍以名称及签名描述解析结果，没有完整声明与 substitution 身份。
2. `expr_helpers.rs` 的 `lower_callee_operand` 将已解析 operator 路径截成末段，
   并在没有解析结果时从表达式拼写生产静态 Symbol。这让后续阶段重新取得解释权。
3. `compile_pipeline/link.rs` 以函数 symbol 建依赖池；`mod.rs` 的
   `callable_identity_table` 对名称排序、去重、编号，再由 `resolve_callable_operands`
   改写调用。晚期编号不是从声明解析贯穿的实例身份。
4. `compile_pipeline/canonical.rs` 从实例编号生成 declaration，完成类型检查后使用
   固定 monomorphic substitution。这不证明泛型调用实例及证据环境已经贯通。

整改以整个调用特性为一个纵向逻辑块：先建立前端唯一声明/实例表，再迁移 HIR、
依赖闭包、MIR、Canonical 与所有消费者，最后删除名称绑定接口。不能为新表保留
旧解析分支、双轨成功类型或过渡开关。迁移器只能机械改写文件与引用，不能补造语义。

验收同时覆盖同名不同 owner、同声明不同泛型实例、静态/实例调用、operator、
函数值、集合普通调用和正式 imports。改变 ABI 展示名不得改变内部绑定；缺身份、
歧义声明、错位实参和未代入类型必须在语义边界失败。源码驱动测试须使用正式
Compiler 入口；手工 MIR 夹具只能证明局部合同，不能代表完整编译流通过。
