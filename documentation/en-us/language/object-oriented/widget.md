# Widget Type (Widget)

The Widget type is a special class type in Valkyrie specifically designed for UI development. It provides high-level abstractions for building modern user interfaces, supporting responsive design, state management, and event handling.

## Basic Widget Definition

### Simple Widget

```valkyrie
# Basic button widget
widget Button {
    # Widget properties
    text: string,
    enabled: bool,
    style: ButtonStyle,
    
    # Event handlers
    on_click: (micro() -> unit)?,
    
    # Constructor
    micro constructor(self, text: string) {
        self.text = text
        self.enabled = true
        self.style = ButtonStyle::default()
        self.on_click = None
    }
    
    # Render method
    micro render(self) -> Element {
        Element::button()
            .text(self.text)
            .enabled(self.enabled)
            .style(self.style)
            .on_click(self.on_click)
    }
    
    # Set click event
    micro on_click(mut self, handler: micro() -> unit) -> Self {
        self.on_click = handler
        self
    }
    
    # Set style
    micro with_style(mut self, style: ButtonStyle) -> Self {
        self.style = style
        self
    }
}
```

### Text Input Widget

```valkyrie
widget TextInput {
    value: string,
    placeholder: string,
    max_length: usize?,
    readonly: bool,
    
    # Event handlers
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
    
    # Set value
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
