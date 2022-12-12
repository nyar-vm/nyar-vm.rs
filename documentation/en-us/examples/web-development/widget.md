# 界面组件 (Widget)：一切的基石

在 Valkyrie 的世界里，UI 并非虚幻的标记，而是由真实的**对象**构建而成的。`widget` 是 Valkyrie 中专门为 UI 设计的类类型，它是整个 UI 系统的逻辑原点。

## 1. 组件的对象本质

每一个界面元素在底层都是一个 `widget` 实例。它通过构造函数初始化，并通过方法链配置属性。

```valkyrie
widget Button {
    text: String,
    enabled: bool,
    style: ButtonStyle,
    on_click: Option<micro() -> ()>,
    
    new(text: String) {
        self.text = text
        self.enabled = true
        self.style = .default
        self.on_click = None
    }
    
    render(self) -> Element {
        Element::button()
            .text(self.text)
            .enabled(self.enabled)
            .style(self.style)
            .on_click(self.on_click)
    }
    
    # 链式配置方法
    on_click(mut self, handler: micro() -> ()) -> Self {
        self.on_click = Some(handler)
        self
    }
}
```

## 2. 布局与组合模式

Widget 系统使用**组合模式**来管理嵌套结构。

### 弹性布局 (FlexBox)
```valkyrie
widget FlexBox {
    children: [FlexChild],
    direction: FlexDirection,
    
    new(direction: FlexDirection = .Row) {
        self.children = []
        self.direction = direction
    }
    
    render(self) -> Element {
        let mut element = Element::div().display(.Flex).flex_direction(self.direction)
        for child in self.children {
            element = element.child(child.widget.render())
        }
        element
    }
    
    add_child(mut self, widget: Box<dyn Widget>) -> Self {
        self.children.push(FlexChild { widget })
        self
    }
}
```

## 3. 状态管理与交互

组件的属性即是它的状态。通过修改字段并请求更新，可以实现响应式交互。

```valkyrie
widget Counter {
    count: i32,
    
    new(initial: i32 = 0) { self.count = initial }
    
    render(self) -> Element {
        FlexBox::new(.Row)
            .add_child(Box::new(Button::new("-").on_click { self.decrement() }))
            .add_child(Box::new(Text::new(self.count.to_string())))
            .add_child(Box::new(Button::new("+").on_click { self.increment() }))
            .render()
    }
    
    increment(mut self) { self.count += 1; self.request_update() }
    decrement(mut self) { self.count -= 1; self.request_update() }
}
```

## 4. 高级特性

### 响应式设计 (Responsive)
利用组件的生命周期和方法，可以轻松实现媒体查询：
```valkyrie
widget Responsive {
    current_breakpoint: String,
    children: {String: Box<dyn Widget>},
    
    # 根据窗口大小更新 current_breakpoint 并触发 render()
    update_width(mut self, width: f32) { ... }
}
```

### 虚拟滚动与模态框
这些高级组件同样基于 Widget 类实现，将复杂的 DOM 操作封装在对象方法中。

---

## 魔法的真相：对象层级即 UI 结构

为什么 Widget 系统让人感到自然？因为它回归了软件工程最经典的**组合模式 (Composite Pattern)**。

当你在屏幕上看到一个复杂的界面时，你在内存中拥有的是一颗完整、透明的对象树。这种设计保证了 UI 系统具有极高的可预测性：
- **没有黑盒**：每一个组件都是一个可以被调试、扩展的普通对象。
- **配置即方法**：流式接口 (Fluent Interface) 让对象配置读起来像自然语言。
- **单一真理来源**：UI 的视觉嵌套直接映射为对象的组合嵌套。

这种“对象化”的基石，为上层的语法简化（如 V-Grammar 和 X-Grammar）提供了最坚实的逻辑保障。
