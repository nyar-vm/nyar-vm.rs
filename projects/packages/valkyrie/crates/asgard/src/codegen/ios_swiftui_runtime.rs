//! iOS / macOS SwiftUI `AsgardSwiftUiRuntime.swift`：ASGARDNT 加载 + UiHost ABI。

use crate::codegen::{HOST_NATIVE_MAGIC, UI_BIN_MAGIC, magic_bytes_literal};

/// SwiftUI 运行时配置（iOS IPA vs 桌面侧车）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwiftUiProfile {
    /// iOS：`@main` + `Bundle.main.executableURL`。
    Ios,
    /// macOS 桌面：无 `@main`，由宿主读 manifest 指向的 exe。
    Desktop,
}

/// 生成含 Asgard UI 解码器的 SwiftUI 运行时（iOS）。
pub fn generate_ios_swiftui_runtime() -> String {
    generate_swiftui_runtime(SwiftUiProfile::Ios)
}

/// 按 profile 生成 SwiftUI 运行时。
pub fn generate_swiftui_runtime(profile: SwiftUiProfile) -> String {
    let ui_magic = magic_bytes_literal(UI_BIN_MAGIC);
    let native_magic = magic_bytes_literal(HOST_NATIVE_MAGIC);
    let app_entry = match profile {
        SwiftUiProfile::Ios => IOS_APP_ENTRY,
        SwiftUiProfile::Desktop => DESKTOP_APP_ENTRY,
    };
    format!(
        r#"import SwiftUI
import Darwin

private let asgardUiMagic: [UInt8] = [{ui_magic}]
private let asgardNativeMagic: [UInt8] = [{native_magic}]

typealias AsgardInvokeExportFn = @convention(c) (UnsafePointer<CChar>) -> Void

struct AsgardBinding: Identifiable {{
    let id = UUID()
    let name: String
    let initValue: String
    let reactive: Bool
    let valueType: UInt8
}}

struct AsgardAttr {{
    let name: String
    let isEvent: Bool
    let isProp: Bool
    let value: String
}}

struct AsgardNode {{
    let kind: UInt8
    var tag: String = ""
    var attrs: [AsgardAttr] = []
    var children: [AsgardNode] = []
    var textParts: [String] = []
    var cond: String = ""
    var loopItems: String = ""
    var loopItemVar: String = ""
}}

struct AsgardComponent: Identifiable {{
    let id = UUID()
    let route: String
    let name: String
    let bindings: [AsgardBinding]
    let nodes: [AsgardNode]
}}

@MainActor
final class AsgardSwiftUiRuntime: ObservableObject {{
    static let shared = AsgardSwiftUiRuntime()

    @Published private(set) var components: [AsgardComponent] = []
    @Published var reactive: [String: String] = [:]

    func loadFromExecutable(_ bytes: Data) {{
        AsgardHostBridge.loadNativeFromExecutable(bytes)
        guard let section = Self.findAsgardUiSection(bytes) else {{ return }}
        components = Self.decodeAsgardUi(section)
        mountAll()
    }}

    func loadFromProduct(_ bytes: Data) {{
        loadFromExecutable(bytes)
    }}

    func mount(_ component: AsgardComponent) {{
        for binding in component.bindings where binding.reactive {{
            reactive[binding.name] = binding.initValue.trimmingCharacters(in: .whitespacesAndNewlines)
        }}
    }}

    func patch(key: String, value: String) {{
        reactive[key] = value
    }}

    func resolveCallExport(_ name: String) -> String {{
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.hasPrefix("awsl_call_") ? trimmed : "awsl_call_" + trimmed
    }}

    func on_event(_ name: String) {{
        AsgardHostBridge.invokeExport(resolveCallExport(name))
    }}

    func dispatchEvent(_ name: String) {{
        on_event(name)
    }}

    private func mountAll() {{
        components.forEach {{ mount($0) }}
    }}

    static func findAsgardUiSection(_ bytes: Data) -> Data? {{
        findSection(bytes, magic: asgardUiMagic)
    }}

    static func findAsgardNativeSection(_ bytes: Data) -> Data? {{
        findSection(bytes, magic: asgardNativeMagic)
    }}

    private static func findSection(_ bytes: Data, magic: [UInt8]) -> Data? {{
        let arr = [UInt8](bytes)
        let magicLen = magic.count
        var idx = 0
        var last: Data?
        while idx + magicLen + 4 <= arr.count {{
            var matched = true
            for i in 0..<magicLen {{
                if arr[idx + i] != magic[i] {{ matched = false; break }}
            }}
            if !matched {{ idx += 1; continue }}
            let len = readU32(arr, idx + magicLen)
            let start = idx + magicLen + 4
            let end = start + Int(len)
            if end > arr.count {{ break }}
            last = Data(arr[start..<end])
            idx = end
        }}
        return last
    }}

    static func decodeAsgardUi(_ bytes: Data) -> [AsgardComponent] {{
        let arr = [UInt8](bytes)
        for i in 0..<asgardUiMagic.count {{
            precondition(arr[i] == asgardUiMagic[i], "invalid Asgard UI wire magic")
        }}
        var offset = asgardUiMagic.count
        var version: UInt8 = 1
        if offset < arr.count && arr[offset] == 0x02 {{
            version = 0x02
            offset += 1
        }}
        let count = Int(readU32(arr, offset))
        offset += 4
        var out: [AsgardComponent] = []
        for _ in 0..<count {{
            let route = readString(arr, offset); offset = route.1
            let name = readString(arr, offset); offset = name.1
            if version == 0x02 {{
                offset = skipAbi(arr, offset)
            }}
            let bindings = readBindings(arr, offset); offset = bindings.1
            let nodes = readIr(arr, offset); offset = nodes.1
            out.append(AsgardComponent(route: route.0, name: name.0, bindings: bindings.0, nodes: nodes.0))
        }}
        return out
    }}

    private static func skipAbi(_ bytes: [UInt8], _ offset: Int) -> Int {{
        var cursor = offset
        var count = Int(readU32(bytes, cursor)); cursor += 4
        for _ in 0..<count {{
            let prop = readString(bytes, cursor); cursor = prop.1
            cursor += 1
            let flags = bytes[cursor]; cursor += 1
            if flags & 2 != 0 {{
                let def = readString(bytes, cursor); cursor = def.1
            }}
        }}
        count = Int(readU32(bytes, cursor)); cursor += 4
        for _ in 0..<count {{
            let event = readString(bytes, cursor); cursor = event.1
            let paramCount = Int(readU32(bytes, cursor)); cursor += 4
            for _ in 0..<paramCount {{
                let param = readString(bytes, cursor); cursor = param.1
                cursor += 1
            }}
        }}
        return cursor
    }}

    private static func readU32(_ bytes: [UInt8], _ offset: Int) -> UInt32 {{
        UInt32(bytes[offset]) | (UInt32(bytes[offset + 1]) << 8) | (UInt32(bytes[offset + 2]) << 16) | (UInt32(bytes[offset + 3]) << 24)
    }}

    private static func readString(_ bytes: [UInt8], _ offset: Int) -> (String, Int) {{
        let len = Int(readU32(bytes, offset))
        let start = offset + 4
        let end = start + len
        let text = String(bytes: bytes[start..<end], encoding: .utf8) ?? ""
        return (text, end)
    }}

    private static func readBindings(_ bytes: [UInt8], _ offset: Int) -> ([AsgardBinding], Int) {{
        let count = Int(readU32(bytes, offset))
        var cursor = offset + 4
        var out: [AsgardBinding] = []
        for _ in 0..<count {{
            let name = readString(bytes, cursor); cursor = name.1
            let initVal = readString(bytes, cursor); cursor = initVal.1
            let reactive = bytes[cursor] != 0
            let valueType = bytes[cursor + 1]
            cursor += 2
            out.append(AsgardBinding(name: name.0, initValue: initVal.0, reactive: reactive, valueType: valueType))
        }}
        return (out, cursor)
    }}

    private static func readIr(_ bytes: [UInt8], _ offset: Int) -> ([AsgardNode], Int) {{
        let count = Int(readU32(bytes, offset))
        var cursor = offset + 4
        var out: [AsgardNode] = []
        for _ in 0..<count {{
            let kind = bytes[cursor]; cursor += 1
            switch kind {{
            case 1:
                let tag = readString(bytes, cursor); cursor = tag.1
                let _nodeKind = bytes[cursor]; cursor += 1
                let attrs = readAttrs(bytes, cursor); cursor = attrs.1
                let children = readIr(bytes, cursor); cursor = children.1
                out.append(AsgardNode(kind: kind, tag: tag.0, attrs: attrs.0, children: children.0))
            case 2:
                let parts = readTextParts(bytes, cursor); cursor = parts.1
                out.append(AsgardNode(kind: kind, textParts: parts.0))
            case 3:
                let cond = readString(bytes, cursor); cursor = cond.1
                let thenBranch = readIr(bytes, cursor); cursor = thenBranch.1
                let elseBranch = readIr(bytes, cursor); cursor = elseBranch.1
                out.append(AsgardNode(kind: kind, cond: cond.0, children: thenBranch.0 + elseBranch.0))
            case 4:
                let items = readString(bytes, cursor); cursor = items.1
                let itemVar = readString(bytes, cursor); cursor = itemVar.1
                let body = readIr(bytes, cursor); cursor = body.1
                out.append(AsgardNode(kind: kind, loopItems: items.0, loopItemVar: itemVar.0, children: body.0))
            default: break
            }}
        }}
        return (out, cursor)
    }}

    private static func readAttrs(_ bytes: [UInt8], _ offset: Int) -> ([AsgardAttr], Int) {{
        let count = Int(readU32(bytes, offset))
        var cursor = offset + 4
        var out: [AsgardAttr] = []
        for _ in 0..<count {{
            let name = readString(bytes, cursor); cursor = name.1
            let isEvent = bytes[cursor] != 0
            let isProp = bytes[cursor + 1] != 0
            let valueKind = bytes[cursor + 2]
            cursor += 3
            let value: String
            if valueKind == 1 || valueKind == 2 {{
                let text = readString(bytes, cursor); cursor = text.1
                value = text.0
            }} else {{
                let parts = readTextParts(bytes, cursor); cursor = parts.1
                value = parts.0.joined()
            }}
            out.append(AsgardAttr(name: name.0, isEvent: isEvent, isProp: isProp, value: value))
        }}
        return (out, cursor)
    }}

    private static func readTextParts(_ bytes: [UInt8], _ offset: Int) -> ([String], Int) {{
        let count = Int(readU32(bytes, offset))
        var cursor = offset + 4
        var out: [String] = []
        for _ in 0..<count {{
            let kind = bytes[cursor]; cursor += 1
            let text = readString(bytes, cursor); cursor = text.1
            out.append(kind == 1 ? text.0 : "{{\(text.0)}}")
        }}
        return (out, cursor)
    }}
}}

struct AsgardRootView: View {{
    @ObservedObject private var runtime = AsgardSwiftUiRuntime.shared

    var body: some View {{
        VStack(alignment: .leading, spacing: 8) {{
            ForEach(runtime.components) {{ component in
                ForEach(Array(component.nodes.enumerated()), id: \.offset) {{ _, node in
                    AsgardNodeView(node: node, reactive: runtime.reactive) {{ event in
                        runtime.on_event(event)
                    }}
                }}
            }}
        }}
        .padding()
    }}
}}

struct AsgardNodeView: View {{
    let node: AsgardNode
    let reactive: [String: String]
    let onEvent: (String) -> Void

    var body: some View {{
        switch node.kind {{
        case 1:
            switch node.tag {{
            case "Column":
                VStack(alignment: .leading) {{
                    ForEach(Array(node.children.enumerated()), id: \.offset) {{ _, child in
                        AsgardNodeView(node: child, reactive: reactive, onEvent: onEvent)
                    }}
                }}
            case "Text":
                Text(resolveText(node.textParts))
            case "Button":
                let label = node.children.first?.textParts.joined() ?? "Button"
                let event = node.attrs.first(where: {{ $0.isEvent }})?.value ?? ""
                Button(label) {{ if !event.isEmpty {{ onEvent(event) }} }}
            default:
                Text(node.tag)
            }}
        case 2:
            Text(resolveText(node.textParts))
        case 3:
            if evalCond(node.cond) {{
                ForEach(Array(node.children.enumerated()), id: \.offset) {{ _, child in
                    AsgardNodeView(node: child, reactive: reactive, onEvent: onEvent)
                }}
            }}
        case 4:
            ForEach(Array(node.children.enumerated()), id: \.offset) {{ _, child in
                AsgardNodeView(node: child, reactive: reactive, onEvent: onEvent)
            }}
        default:
            EmptyView()
        }}
    }}

    private func resolveText(_ parts: [String]) -> String {{
        parts.map {{ part in
            if part.hasPrefix("{{"), part.hasSuffix("}}") {{
                let key = String(part.dropFirst().dropLast())
                return reactive[key] ?? part
            }}
            return part
        }}.joined()
    }}

    private func evalCond(_ cond: String) -> Bool {{
        let key = cond.trimmingCharacters(in: .whitespacesAndNewlines)
        if let v = reactive[key] {{ return v == "true" || v == "1" }}
        return key == "true"
    }}
}}

{app_entry}

enum AsgardHostBridge {{
    private static var invokeFn: AsgardInvokeExportFn?
    private static var nativeHandle: UnsafeMutableRawPointer?

    static func loadNativeFromExecutable(_ bytes: Data) {{
        guard let native = AsgardSwiftUiRuntime.findAsgardNativeSection(bytes) else {{
            fatalError("product missing ASGARDNT section")
        }}
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("asgard_logic")
        try? native.write(to: url)
        guard let handle = dlopen(url.path, RTLD_NOW) else {{
            fatalError(String(cString: dlerror()))
        }}
        nativeHandle = handle
        guard let sym = dlsym(handle, "asgard_invoke_export") else {{
            fatalError("missing asgard_invoke_export")
        }}
        invokeFn = unsafeBitCast(sym, to: AsgardInvokeExportFn.self)
    }}

    static func invokeExport(_ name: String) {{
        guard let fn = invokeFn else {{ return }}
        name.withCString {{ fn($0) }}
    }}

    static func patchFromNative(key: String, value: String) {{
        Task {{ @MainActor in AsgardSwiftUiRuntime.shared.patch(key: key, value: value) }}
    }}
}}

@_cdecl("asgard_patch_native_swift")
func asgard_patch_native_swift(_ key: UnsafePointer<CChar>, _ value: UnsafePointer<CChar>) {{
    let k = String(cString: key)
    let v = String(cString: value)
    AsgardHostBridge.patchFromNative(key: k, value: v)
}}
"#,
        app_entry = app_entry
    )
}

const IOS_APP_ENTRY: &str = r#"@main
struct AsgardApp: App {
    init() {
        if let url = Bundle.main.executableURL, let data = try? Data(contentsOf: url) {
            AsgardSwiftUiRuntime.shared.loadFromExecutable(data)
        }
    }
    var body: some Scene {
        WindowGroup { AsgardRootView() }
    }
}
"#;

const DESKTOP_APP_ENTRY: &str = r#"// Desktop: no main entry — host loads product bytes via loadFromProduct(exeData).
"#;
#[cfg(test)]
#[path = "../../tests/path_wired/codegen_ios_swiftui_runtime.rs"]
mod codegen_ios_swiftui_runtime_tests;
