# XML 语法 (X-Grammar)

在掌握了 [Widget](./widget.md) 的对象模型和 [V-Grammar](./v-grammar.md) 的闭包语法后，X-Grammar 为我们提供了 UI 逻辑的**视觉投影**。

它没有任何类似 Vue 或 Svelte 的“特殊指令”（如 `v-on` 或 `on:`），因为它本质上只是将 XML 属性 1:1 地映射到底层 Widget 对象的字段或方法上。

## 1. 基础语法与属性绑定

X-Grammar 使用类 XML 的标签来描述 UI 结构。所有的交互和数据流动都通过统一的属性绑定实现：

- **立即属性 `( )`**：用于需要立即计算并赋值的场景。
    - **标识符简写**：`name=value`。
    - **表达式求值**：`name=(expression)`。
- **闭包属性 `{ }`**：用于传递逻辑块（闭包）。在底层，这通常对应于 Widget 的事件注册方法（如 `on_click`）。

```xml
<div class="container">
    <h1>欢迎来到 Valkyrie</h1>
    
    <!-- disabled 接受布尔值，on_click 接受闭包 -->
    <button disabled=(count >= 10) on_click={ count += 1 }>
        (if (count == 0) { "开始" } else { "继续" })
    </button>
    
    <!-- 事件转发：本质上就是将父组件传入的闭包 prop 传递给子组件 -->
    <CustomWidget on_click=on_click />
    
    <p>当前进度：(progress)%</p>
</div>
```

## 2. 逻辑标签

逻辑标签是 Valkyrie 原生控制流在视觉上的直接映射，同样遵循无歧义的括号原则。

### 条件渲染 (`<if>`)
```xml
<if (count > 5)>
    <p>计数已过半</p>
<else/>
    <p>继续努力</p>
</if>
```

### 模式匹配 (`<match>`)
```xml
<match (user.role)>
    <case "admin">  <badge>管理员</badge> </case>
    <case "user">   <badge>普通用户</badge> </case>
    <else>          <badge>访客</badge>    </else>
</match>
```

### 循环迭代 (`<for>`)
```xml
<for (item, index) in (list)>
    <li key=index>(item.name)</li>
<else/>
    <p>列表为空</p>
</for>
```

---

## 3. 扩展：单文件组件 (SFC)

单文件组件 (Single File Component) 是 X-Grammar 的一种应用模式。它通过 `<script>` 块定义逻辑上下文，使模板可以直接访问其中的符号。

```xml
<div class="container">
    <h1>Hello, (name)</h1>
    <!-- handleClick 是一个函数，作为闭包传递 -->
    <button on_click=handleClick>
        点击次数: (count)
    </button>
</div>

<script>
let name = "Valkyrie";
let count = 0;

micro handleClick() {
    count += 1;
}
</script>

<style>
# auto scoped
</style>
```

## 4. 语法对比

X-Grammar 没有任何“魔法指令”，一切皆是属性与闭包的组合。

| X-Grammar | 语义 | V-Grammar 等效代码 |
| :--- | :--- | :--- |
| `name=(val)` | 属性赋值 (立即) | `.name(val)` |
| `name={...}` | 闭包传递 (延迟) | `.name({ ... })` |
| `<if (cond)>` | 条件分支 | `if cond { ... }` |
| `<match (val)>` | 模式匹配 | `match val { ... }` |
| `<for (i) in (L)>` | 循环迭代 | `for i in L { ... }` |

---

## 魔法的真相：逻辑的视觉投影

### 1. 零指令设计
Valkyrie 不需要 `v-bind` 或 `on:`，因为 X-Grammar 信任底层的对象模型。如果一个 Widget 有 `on_click` 方法，你就在 XML 里写 `on_click`。这种“零心智负担”的设计让 UI 描述回归到了编程语言的最基本概念：**对象初始化与方法调用**。

### 2. 括号的力量
通过 `()` 和 `{}`，X-Grammar 在编译阶段就明确了“值”与“逻辑”的区别。这种区分不仅消除了 XML 解析的歧义，还允许编译器生成最优化的底层代码，无需在运行时去猜测一个属性到底是数据还是回调。

### 3. 静态转换：消失的开销
所有的 X-Grammar 语法在编译阶段都会被“拍扁”成最高效的原生方法链。在运行时，有的只是经过极致优化的 Widget 对象操作。

这种设计让 Valkyrie 既拥有了 XML 的直观，又彻底消除了传统前端框架中“指令集”带来的学习成本和运行负担。
