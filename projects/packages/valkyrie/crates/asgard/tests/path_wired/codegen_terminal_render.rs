//! Migrated from `src/codegen/terminal_render.rs` (`tests`).

use super::*;

use crate::awsl::{LoweringOptions, lower_component};
use oak_awsl::AwslParser;

fn lower_source(source: &str) -> LoweredComponent {
    let root = AwslParser::parse_root(source).expect("parse");
    lower_component(&root, "Test", "test.awsl", &LoweringOptions::default())
}

fn row_text(result: &TerminalRenderResult, row: i32) -> String {
    let mut chars: Vec<(i32, char)> = result.cells.iter().filter(|c| c.row == row).map(|c| (c.col, c.ch)).collect();
    chars.sort_by_key(|(col, _)| *col);
    let mut out = String::new();
    let mut last = 0i32;
    for (col, ch) in chars {
        while last < col {
            out.push(' ');
            last += 1;
        }
        out.push(ch);
        last = col + 1;
    }
    out
}

#[test]
fn column_stacks_children_vertically() {
    let component = lower_source("<widget><Column><Text>First</Text><Text>Second</Text></Column></widget>");
    let result = render_terminal_ir(&component, &serde_json::json!({}), 10, 40);
    let row0 = row_text(&result, 0);
    let row1 = row_text(&result, 1);
    assert!(row0.contains("First"), "row0={}", row0);
    assert!(row1.contains("Second"), "row1={}", row1);
}

#[test]
fn row_places_children_horizontally() {
    let component = lower_source("<widget><Row><Text>AB</Text><Text>CD</Text></Row></widget>");
    let result = render_terminal_ir(&component, &serde_json::json!({}), 5, 40);
    let a_col = result.cells.iter().find(|c| c.ch == 'A').map(|c| c.col);
    let c_col = result.cells.iter().find(|c| c.ch == 'C').map(|c| c.col);
    let (Some(a), Some(c)) = (a_col, c_col)
    else {
        panic!("expected A and C cells, got {:?} {:?}", a_col, c_col);
    };
    assert!(a < c, "A at col {} should be before C at col {}", a, c);
}

#[test]
fn box_draws_border_with_plus_corners() {
    let component = lower_source("<widget><Box><Text>Hi</Text></Box></widget>");
    let result = render_terminal_ir(&component, &serde_json::json!({}), 5, 10);
    assert!(result.cells.iter().any(|c| c.ch == '+'), "expected + corner in cells");
}

#[test]
fn text_writes_characters_to_cells() {
    let component = lower_source("<widget><Text>Hello</Text></widget>");
    let result = render_terminal_ir(&component, &serde_json::json!({}), 5, 40);
    let chars: String = result.cells.iter().filter(|c| c.row == 0).map(|c| c.ch).collect();
    assert!(chars.contains("Hello"), "chars={}", chars);
}

#[test]
fn button_registers_focusable_with_event_name() {
    let component = lower_source(r#"<widget><Button @click="on_tap">Click</Button></widget>"#);
    let result = render_terminal_ir(&component, &serde_json::json!({}), 5, 40);
    assert_eq!(result.focusables.len(), 1);
    assert_eq!(result.focusables[0].event_name, "on_tap");
    assert_eq!(result.focusables[0].label, "Click");
    assert_eq!(result.focusables[0].kind, FocusableKind::Button);
}

#[test]
fn list_items_register_as_item_kind_vertically() {
    let component = lower_source(
        r#"<widget><List>
            <Item @click="on_a">Apple</Item>
            <Item @click="on_b">Banana</Item>
            <Item @click="on_c">Cherry</Item>
        </List></widget>"#,
    );
    let result = render_terminal_ir(&component, &serde_json::json!({}), 10, 40);
    assert_eq!(result.focusables.len(), 3, "list should register 3 items");
    for (i, f) in result.focusables.iter().enumerate() {
        assert_eq!(f.kind, FocusableKind::Item, "item {} should be Item kind", i);
    }
    assert_eq!(result.focusables[0].label, "Apple");
    assert_eq!(result.focusables[1].label, "Banana");
    assert_eq!(result.focusables[2].label, "Cherry");
    assert_eq!(result.focusables[0].event_name, "on_a");
    let rows: Vec<i32> = result.focusables.iter().map(|f| f.layout.row).collect();
    assert!(rows[0] < rows[1] && rows[1] < rows[2], "items must stack vertically: {:?}", rows);
}

#[test]
fn if_renders_then_branch_when_true() {
    let component = lower_source("<widget><if show><Text>Yes</Text><else/><Text>No</Text></if></widget>");
    let result = render_terminal_ir(&component, &serde_json::json!({ "show": true }), 5, 40);
    let all: String = result.cells.iter().map(|c| c.ch).collect();
    assert!(all.contains("Yes"), "all={}", all);
    assert!(!all.contains("No"), "all={}", all);
}

#[test]
fn if_renders_else_branch_when_false() {
    let component = lower_source("<widget><if show><Text>Yes</Text><else/><Text>No</Text></if></widget>");
    let result = render_terminal_ir(&component, &serde_json::json!({ "show": false }), 5, 40);
    let all: String = result.cells.iter().map(|c| c.ch).collect();
    assert!(all.contains("No"), "all={}", all);
    assert!(!all.contains("Yes"), "all={}", all);
}

#[test]
fn loop_renders_body_for_each_item() {
    let component = lower_source("<widget><loop item in items><Text>{item}</Text></loop></widget>");
    let result = render_terminal_ir(&component, &serde_json::json!({ "items": ["a", "b", "c"] }), 10, 40);
    let row0 = row_text(&result, 0);
    let row1 = row_text(&result, 1);
    let row2 = row_text(&result, 2);
    assert!(row0.contains("a"), "row0={}", row0);
    assert!(row1.contains("b"), "row1={}", row1);
    assert!(row2.contains("c"), "row2={}", row2);
}

#[test]
fn encode_focusables_roundtrip() {
    let component = lower_source(r#"<widget><Column><Button @click="on_a">A</Button><Button @click="on_b">B</Button></Column></widget>"#);
    let result = render_terminal_ir(&component, &serde_json::json!({}), 10, 40);
    assert_eq!(result.focusables.len(), 2);
    let bytes = encode_focusables(&result.focusables);
    assert!(bytes.starts_with(FOCUSABLES_MAGIC));
    let count = u32::from_le_bytes(bytes[9..13].try_into().unwrap());
    assert_eq!(count, 2);
    // 每条记录：row(4)+col(4)+width(4)+height(4)+kind(1)，首条 kind 在 offset 13+16=29
    assert_eq!(bytes[29], FocusableKind::Button.tag(), "first focusable kind must be Button(0)");
}

#[test]
fn encode_focusables_empty() {
    let bytes = encode_focusables(&[]);
    assert!(bytes.starts_with(FOCUSABLES_MAGIC));
    let count = u32::from_le_bytes(bytes[9..13].try_into().unwrap());
    assert_eq!(count, 0);
}

#[test]
fn mixed_buttons_and_items_both_kinds_present() {
    let component = lower_source(
        r#"<widget><Column>
            <Row>
                <Button @click="on_dec">-1</Button>
                <Button @click="on_inc">+1</Button>
            </Row>
            <List>
                <Item @click="on_a">Apple</Item>
                <Item @click="on_b">Banana</Item>
            </List>
        </Column></widget>"#,
    );
    let result = render_terminal_ir(&component, &serde_json::json!({}), 20, 60);
    let buttons = result.focusables.iter().filter(|f| f.kind == FocusableKind::Button).count();
    let items = result.focusables.iter().filter(|f| f.kind == FocusableKind::Item).count();
    assert_eq!(buttons, 2, "expected 2 buttons, got {}", buttons);
    assert_eq!(items, 2, "expected 2 items, got {}", items);
    let bytes = encode_focusables(&result.focusables);
    assert!(bytes.starts_with(FOCUSABLES_MAGIC));
    // 首条 Button kind=0 在 offset 29（9 magic + 4 count + 16 layout）
    assert_eq!(bytes[29], 0);
}
