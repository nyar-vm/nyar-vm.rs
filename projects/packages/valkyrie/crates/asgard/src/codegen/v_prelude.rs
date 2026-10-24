//! AWSL → WASM 编译用的 DOM `js_builtin` 前导声明。

/// DOM 宿主绑定前导（与 asgard `dom.v` 对齐，供 WASM 侧调用）。
pub const DOM_PRELUDE: &str = r#"
[js_builtin("document.createElement")]
micro dom_create_element(tag: utf8): i32

[js_builtin("document.createTextNode")]
micro dom_create_text(text: utf8): i32

[js_builtin("Element.setAttribute")]
micro dom_set_attr(handle: i32, name: utf8, value: utf8)

[js_builtin("Element.appendChild")]
micro dom_append(parent: i32, child: i32)

[js_builtin("Element.textContent")]
micro dom_set_text(handle: i32, text: utf8)

[js_builtin("Element.classList.add")]
micro dom_class_add(handle: i32, token: utf8)

[js_builtin("Element.classList.remove")]
micro dom_class_remove(handle: i32, token: utf8)

[js_builtin("Element.classList.toggle")]
micro dom_class_toggle(handle: i32, token: utf8)

[js_builtin("Element.addEventListener")]
micro dom_add_event(handle: i32, event: utf8, callback: i32)

[js_builtin("Element.removeEventListener")]
micro dom_remove_event(handle: i32, event: utf8, callback: i32)

[js_builtin("Element.removeChild")]
micro dom_remove_child(parent: i32, child: i32)

[js_builtin("Element.insertBefore")]
micro dom_insert_before(parent: i32, child: i32, before: i32)

[js_builtin("Element.replaceChild")]
micro dom_replace_child(parent: i32, child: i32, old: i32)

# AWSL 页面 script 依赖的数据 API（项目 data.v 未接入时的占位）
micro get_all_posts(): list {
    return []
}

micro get_all_tags(): list {
    return []
}

micro get_post_by_slug(slug: utf8): i32 {
    let zero: i32 = 0
    return zero
}

micro get_comments(post_slug: utf8): list {
    return []
}

micro add_comment(post_slug: utf8, author: utf8, content: utf8): i32 {
    let zero: i32 = 0
    return zero
}

micro length(lst: list): i32 {
    let zero: i32 = 0
    return zero
}

micro use_params(): list {
    return []
}

# demo.asgard.todo 数据 API 占位
micro get_todos(): list { return [] }
micro get_all_todos(): list { return [] }
micro get_filter(): utf8 { return "all" }
micro set_filter(f: utf8) { return }
micro add_todo(text: utf8) { return }
micro toggle_todo(id: i32) { return }
micro remove_todo(id: i32) { return }
micro clear_done() { return }
micro count_active(): i32 { let zero: i32 = 0; return zero }
micro count_done(): i32 { let zero: i32 = 0; return zero }
micro count_all(): i32 { let zero: i32 = 0; return zero }

"#;
