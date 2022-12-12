# 原生 UI 语法 (V-Grammar)：闭包的优雅封装

在理解了 [Widget](./widget.md) 的对象本质后，你会发现手动创建和嵌套这些对象有时会显得繁琐。V-Grammar 应运而生，它利用 Valkyrie 的语言特性，为对象的构建提供了一套极其自然的声明式语法。

## 1. 声明式构建

V-Grammar 将复杂的方法链调用简化为了直观的块状结构。

```valkyrie
div {
    class("container")
    h1 { "欢迎来到 Valkyrie" }
    p { "这只是 Widget 对象的一种快捷写法。" }
}
```

## 2. 属性与数据绑定

属性通常作为块内部的函数调用。对于动态数据，可以使用 `bind()` 函数或直接在字符串中使用插值。

```valkyrie
button {
    disabled(isLoading)
    on_click(handleClick)
    
    if count == 0 { "开始" } else { "继续" }
}
```

## 3. 控制流与模式匹配

由于 V-Grammar 本质上就是执行代码，它可以毫无阻碍地使用 Valkyrie 所有的控制流工具。

### 条件与分支
```valkyrie
div {
    if condition {
        p { "条件为真" }
    } else {
        p { "条件为假" }
    }
}
```

### 模式匹配 (Match)
对于复杂的 UI 状态，`match` 表达式提供了强大的处理能力：
```valkyrie
div {
    match user.status {
        case online:  span { "在线" }
        case offline: span { "离线" }
        else:          span { "未知" }
    }
}
```

### 循环迭代
```valkyrie
ul {
    for item, index in list {
        li {
            key(index)
            item.name
        }
    }
}
```

## 4. 原始文本渲染

对于多行或包含特殊字符（如 `{}`）的文本，可以使用三引号字符串避免解析：

```valkyrie
div {
    r"""
    这段文字中的 {brackets} 不会被解析，
    且支持多行。
    """
}
```

---

## 魔法的真相：闭包的自然演化

V-Grammar 的“魔法感”源于它对 **尾随闭包 (Trailing Closures)** 和 **隐式接收者 (Implicit Receivers)** 的极致利用。

### 1. 从方法到块
当你写下 `div { ... }` 时，编译器实际上将其识别为一个接受闭包的函数调用。这个闭包内部的代码被用来配置新创建的 `div` 对象。

### 2. 上下文的自动传递
在闭包内部，当前的 Widget 对象成为了隐式的 `self`。这就是为什么你可以直接调用 `class()` 而不需要写 `element.class()`。

### 3. 消解鸿沟
这种设计消除了“编写业务逻辑”与“描述界面结构”之间的鸿沟。它不是一种被强加的模板语言，而是利用闭包特性对对象操作进行的一次自然升华。在 V-Grammar 中，你依然在操作 Widget 对象，只是这种操作变得前所未有的流畅。
