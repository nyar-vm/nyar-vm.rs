# 介面組件型別 (Widget)

介面組件型別是 Valkyrie 中專門為 UI 開發設計的特殊類別型別。它提供了構建現代使用者介面的高級抽象，支援響應式設計、狀態管理和事件處理。

## 基本組件定義

### 簡單組件

```valkyrie
# 基本按鈕組件
widget Button {
    # 組件屬性
    text: string,
    enabled: bool,
    style: ButtonStyle,
    
    # 事件處理器
    on_click: (micro() -> unit)?,
    
    # 建構函式
    micro constructor(self, text: string) {
        self.text = text
        self.enabled = true
        self.style = ButtonStyle::default()
        self.on_click = None
    }
    
    # 渲染方法
    micro render(self) -> Element {
        Element::button()
            .text(self.text)
            .enabled(self.enabled)
            .style(self.style)
            .on_click(self.on_click)
    }
    
    # 設置點擊事件
    micro on_click(mut self, handler: micro() -> unit) -> Self {
        self.on_click = handler
        self
    }
    
    # 設置樣式
    micro with_style(mut self, style: ButtonStyle) -> Self {
        self.style = style
        self
    }
}
```

### 文本輸入組件

```valkyrie
widget TextInput {
    value: string,
    placeholder: string,
    max_length: usize?,
    readonly: bool,
    
    # 事件處理器
    on_change: (micro(string) -> unit)?,
    on_focus: (micro() -> unit)?,
    on_blur: (micro() -> unit)?,
    
    micro constructor(self, placeholder: string = "") {
        self.value = ""
        self.placeholder = placeholder
        self.max_length = None
        self.readonly = false
        self.on_change = None
        self.on_focus = None
        self.on_blur = None
    }
    
    micro render(self) -> Element {
        Element::input()
            .value(self.value)
            .placeholder(self.placeholder)
            .max_length(self.max_length)
            .readonly(self.readonly)
            .on_change(self.on_change)
            .on_focus(self.on_focus)
            .on_blur(self.on_blur)
    }
    
    # 設置值
    micro set_value(mut self, value: string) {
        match self.max_length {
            case max_len:
                if value.len() > max_len {
                    return
                }
            case None: {}
        }
        
        self.value = value
        
        match self.on_change {
            case handler: handler(self.value.clone())
            case None: {}
        }
    }
}
```
