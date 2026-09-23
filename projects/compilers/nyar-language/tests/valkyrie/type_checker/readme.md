# compiler tests type checker

这里验证类型推断、约束求解、trait / witness 与名义子类型事实。

## 与 `spec/` 的分工

- `type_checker/` 更偏当前实现与局部规则验证。
- `spec/` 更偏语言规范与长期边界护栏。
- 当某条语义规则已经定稿但实现还没补齐时，先在 `spec/` 立测试，再逐步把具体检查收敛到这里。

