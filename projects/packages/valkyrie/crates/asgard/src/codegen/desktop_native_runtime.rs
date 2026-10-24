//! 桌面一手 native runtime（WinUI / SwiftUI / Linux native）：ASGARDUI wire 解码 + mount/patch/on_event。

use crate::codegen::{
    HOST_NATIVE_MAGIC, UI_BIN_MAGIC,
    ios_swiftui_runtime::{SwiftUiProfile, generate_swiftui_runtime},
    magic_bytes_literal,
};

/// 按平台生成桌面 UI provider 运行时模板。
pub fn generate_desktop_native_runtime(platform: &str) -> String {
    match platform {
        "windows" => generate_winui_runtime(),
        "macos" => generate_macos_swiftui_runtime(),
        _ => generate_linux_native_runtime(),
    }
}

/// Runtime 源文件名（相对 dist）。
pub fn desktop_runtime_asset_name(platform: &str) -> &'static str {
    match platform {
        "windows" => "AsgardWinUiRuntime.cs",
        "macos" => "AsgardSwiftUiRuntime.swift",
        _ => "AsgardLinuxNativeRuntime.c",
    }
}

fn generate_linux_native_runtime() -> String {
    let ui_magic = magic_bytes_literal(UI_BIN_MAGIC);
    let native_magic = magic_bytes_literal(HOST_NATIVE_MAGIC);
    format!(
        r#"/* Asgard Linux native runtime — ASGARDNT + ASGARDUI + UiHost ABI */
#include <stdint.h>
#include <string.h>
#include <stdlib.h>
#include <stdio.h>
#include <dlfcn.h>

static const uint8_t ASGARD_UI_MAGIC[8] = {{{ui_magic}}};
static const uint8_t ASGARD_NATIVE_MAGIC[8] = {{{native_magic}}};

extern void asgard_linux_mount(const uint8_t* pkg, uint32_t len);
extern void asgard_linux_patch(const char* key, const char* value);

typedef void (*asgard_invoke_export_fn)(const char*);

static void* g_native_handle = 0;
static asgard_invoke_export_fn g_invoke_export = 0;

static uint32_t read_u32_le(const uint8_t* p) {{
    return (uint32_t)p[0] | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24);
}}

static const uint8_t* find_section(const uint8_t* blob, uint32_t blob_len, const uint8_t* magic, uint32_t magic_len, uint32_t* out_len) {{
    if (!blob || !out_len) return 0;
    *out_len = 0;
    const uint8_t* last = 0;
    uint32_t last_len = 0;
    for (uint32_t i = 0; i + magic_len + 4 <= blob_len; i++) {{
        if (memcmp(blob + i, magic, magic_len) != 0) continue;
        uint32_t len = read_u32_le(blob + i + magic_len);
        if (i + magic_len + 4 + len > blob_len) break;
        last = blob + i + magic_len + 4;
        last_len = len;
        i = i + magic_len + 4 + len - 1;
    }}
    if (last) *out_len = last_len;
    return last;
}}

static const uint8_t* asgard_find_asgard_ui(const uint8_t* blob, uint32_t blob_len, uint32_t* out_len) {{
    return find_section(blob, blob_len, ASGARD_UI_MAGIC, 8, out_len);
}}

static const uint8_t* asgard_find_asgard_native(const uint8_t* blob, uint32_t blob_len, uint32_t* out_len) {{
    return find_section(blob, blob_len, ASGARD_NATIVE_MAGIC, 8, out_len);
}}

static void resolve_call_export(const char* name, char* out, uint32_t out_len) {{
    const char* trimmed = name ? name : "";
    if (strncmp(trimmed, "awsl_call_", 10) == 0) {{
        strncpy(out, trimmed, out_len - 1);
    }} else {{
        snprintf(out, out_len, "awsl_call_%s", trimmed);
    }}
    out[out_len - 1] = 0;
}}

static int asgard_load_native_from_product(const uint8_t* blob, uint32_t blob_len) {{
    uint32_t native_len = 0;
    const uint8_t* native = asgard_find_asgard_native(blob, blob_len, &native_len);
    if (!native || native_len == 0) return -1;
    const char* path = "/tmp/asgard_logic.so";
    FILE* f = fopen(path, "wb");
    if (!f) return -1;
    fwrite(native, 1, native_len, f);
    fclose(f);
    g_native_handle = dlopen(path, RTLD_NOW);
    if (!g_native_handle) return -1;
    g_invoke_export = (asgard_invoke_export_fn)dlsym(g_native_handle, "asgard_invoke_export");
    return g_invoke_export ? 0 : -1;
}}

void asgard_desktop_load(const uint8_t* exe, uint32_t exe_len) {{
    asgard_load_native_from_product(exe, exe_len);
    uint32_t ui_len = 0;
    const uint8_t* ui = asgard_find_asgard_ui(exe, exe_len, &ui_len);
    if (!ui || ui_len == 0) return;
    asgard_linux_mount(ui, ui_len);
}}

void asgard_desktop_mount(const uint8_t* pkg, uint32_t len) {{
    asgard_linux_mount(pkg, len);
}}

void asgard_desktop_patch(const char* key, const char* value) {{
    asgard_linux_patch(key, value);
}}

void asgard_desktop_on_event(const char* name) {{
    char export_name[128];
    resolve_call_export(name, export_name, sizeof(export_name));
    if (g_invoke_export) g_invoke_export(export_name);
}}

void asgard_patch_native(const char* key, const char* value) {{
    asgard_linux_patch(key, value);
}}
"#
    )
}

fn generate_winui_runtime() -> String {
    let ui_magic = magic_bytes_literal(UI_BIN_MAGIC);
    let native_magic = magic_bytes_literal(HOST_NATIVE_MAGIC);
    format!(
        r#"// Asgard WinUI runtime — ASGARDNT + ASGARDUI + UiHost ABI
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

namespace Asgard.Runtime
{{
    public sealed class AsgardBinding
    {{
        public string Name {{ get; set; }} = "";
        public string InitValue {{ get; set; }} = "";
        public bool Reactive {{ get; set; }}
        public byte ValueType {{ get; set; }}
    }}

    public sealed class AsgardAttr
    {{
        public string Name {{ get; set; }} = "";
        public bool IsEvent {{ get; set; }}
        public bool IsProp {{ get; set; }}
        public string Value {{ get; set; }} = "";
    }}

    public sealed class AsgardNode
    {{
        public byte Kind {{ get; set; }}
        public string Tag {{ get; set; }} = "";
        public List<AsgardAttr> Attrs {{ get; set; }} = new();
        public List<AsgardNode> Children {{ get; set; }} = new();
        public List<string> TextParts {{ get; set; }} = new();
        public string Cond {{ get; set; }} = "";
    }}

    public sealed class AsgardComponent
    {{
        public string Route {{ get; set; }} = "";
        public string Name {{ get; set; }} = "";
        public List<AsgardBinding> Bindings {{ get; set; }} = new();
        public List<AsgardNode> Nodes {{ get; set; }} = new();
    }}

    internal static class NativeBridge
    {{
        [DllImport("asgard_logic", EntryPoint = "asgard_invoke_export", CallingConvention = CallingConvention.Cdecl)]
        internal static extern void InvokeExport(IntPtr name);
    }}

    public static class AsgardWinUiRuntime
    {{
        private static readonly byte[] UiMagic = {{ {ui_magic} }};
        private static readonly byte[] NativeMagic = {{ {native_magic} }};
        private static List<AsgardComponent> Components = new();
        private static readonly Dictionary<string, string> Reactive = new();

        public static void LoadFromProduct(byte[] image)
        {{
            LoadNativeFromProduct(image);
            var section = FindAsgardUiSection(image);
            if (section == null) return;
            Components = DecodeAsgardUi(section);
            MountAll();
        }}

        public static void LoadFromPe(byte[] image) => LoadFromProduct(image);

        private static void LoadNativeFromProduct(byte[] product)
        {{
            var native = FindAsgardNativeSection(product);
            if (native == null) throw new InvalidOperationException("product missing ASGARDNT section");
            var path = System.IO.Path.Combine(System.IO.Path.GetTempPath(), "asgard_logic.dll");
            System.IO.File.WriteAllBytes(path, native);
            // NativeBridge resolves after LoadLibrary in full shell; P/Invoke expects asgard_logic.dll on PATH.
        }}

        public static void Mount(AsgardComponent component)
        {{
            foreach (var b in component.Bindings)
                if (b.Reactive) Reactive[b.Name] = b.InitValue.Trim();
        }}

        public static void Patch(string key, string value) => Reactive[key] = value;

        public static void PatchFromNative(string key, string value) => Patch(key, value);

        public static string ResolveCallExport(string name)
        {{
            var trimmed = (name ?? "").Trim();
            return trimmed.StartsWith("awsl_call_") ? trimmed : "awsl_call_" + trimmed;
        }}

        public static void OnEvent(string name)
        {{
            NativeBridge.InvokeExport(Marshal.StringToHGlobalAnsi(ResolveCallExport(name)));
        }}

        public static void DispatchEvent(string name) => OnEvent(name);

        public static byte[]? FindAsgardUiSection(byte[] bytes) => FindSection(bytes, UiMagic);

        public static byte[]? FindAsgardNativeSection(byte[] bytes) => FindSection(bytes, NativeMagic);

        private static byte[]? FindSection(byte[] bytes, byte[] magic)
        {{
            byte[]? last = null;
            for (int i = 0; i + magic.Length + 4 <= bytes.Length; i++)
            {{
                bool matched = true;
                for (int j = 0; j < magic.Length; j++)
                {{
                    if (bytes[i + j] != magic[j]) {{ matched = false; break; }}
                }}
                if (!matched) continue;
                int len = ReadU32(bytes, i + magic.Length);
                int start = i + magic.Length + 4;
                int end = start + len;
                if (end > bytes.Length) break;
                last = bytes[start..end];
                i = end - 1;
            }}
            return last;
        }}

        public static List<AsgardComponent> DecodeAsgardUi(byte[] bytes)
        {{
            for (int i = 0; i < UiMagic.Length; i++)
                if (bytes[i] != UiMagic[i]) throw new InvalidOperationException("invalid Asgard UI wire magic");
            int offset = UiMagic.Length;
            int count = ReadU32(bytes, offset); offset += 4;
            var outList = new List<AsgardComponent>(count);
            for (int c = 0; c < count; c++)
            {{
                var route = ReadString(bytes, ref offset);
                var name = ReadString(bytes, ref offset);
                var bindings = ReadBindings(bytes, ref offset);
                var nodes = ReadIr(bytes, ref offset);
                outList.Add(new AsgardComponent {{ Route = route, Name = name, Bindings = bindings, Nodes = nodes }});
            }}
            return outList;
        }}

        private static void MountAll() {{ foreach (var c in Components) Mount(c); }}

        private static int ReadU32(byte[] bytes, int offset) =>
            bytes[offset] | (bytes[offset + 1] << 8) | (bytes[offset + 2] << 16) | (bytes[offset + 3] << 24);

        private static string ReadString(byte[] bytes, ref int offset)
        {{
            int len = ReadU32(bytes, offset); offset += 4;
            string text = Encoding.UTF8.GetString(bytes, offset, len);
            offset += len;
            return text;
        }}

        private static List<AsgardBinding> ReadBindings(byte[] bytes, ref int offset)
        {{
            int count = ReadU32(bytes, offset); offset += 4;
            var list = new List<AsgardBinding>(count);
            for (int i = 0; i < count; i++)
            {{
                var name = ReadString(bytes, ref offset);
                var init = ReadString(bytes, ref offset);
                bool reactive = bytes[offset] != 0;
                byte valueType = bytes[offset + 1];
                offset += 2;
                list.Add(new AsgardBinding {{ Name = name, InitValue = init, Reactive = reactive, ValueType = valueType }});
            }}
            return list;
        }}

        private static List<AsgardNode> ReadIr(byte[] bytes, ref int offset)
        {{
            int count = ReadU32(bytes, offset); offset += 4;
            var list = new List<AsgardNode>(count);
            for (int i = 0; i < count; i++)
            {{
                byte kind = bytes[offset++];
                var node = new AsgardNode {{ Kind = kind }};
                switch (kind)
                {{
                    case 1:
                        node.Tag = ReadString(bytes, ref offset);
                        offset++;
                        node.Attrs = ReadAttrs(bytes, ref offset);
                        node.Children = ReadIr(bytes, ref offset);
                        break;
                    case 2:
                        node.TextParts = ReadTextParts(bytes, ref offset);
                        break;
                    case 3:
                        node.Cond = ReadString(bytes, ref offset);
                        node.Children = ReadIr(bytes, ref offset);
                        break;
                }}
                list.Add(node);
            }}
            return list;
        }}

        private static List<AsgardAttr> ReadAttrs(byte[] bytes, ref int offset)
        {{
            int count = ReadU32(bytes, offset); offset += 4;
            var list = new List<AsgardAttr>(count);
            for (int i = 0; i < count; i++)
            {{
                var name = ReadString(bytes, ref offset);
                bool isEvent = bytes[offset] != 0;
                bool isProp = bytes[offset + 1] != 0;
                byte valueKind = bytes[offset + 2];
                offset += 3;
                string value;
                if (valueKind == 1 || valueKind == 2) value = ReadString(bytes, ref offset);
                else {{ var parts = ReadTextParts(bytes, ref offset); value = string.Join("", parts); }}
                list.Add(new AsgardAttr {{ Name = name, IsEvent = isEvent, IsProp = isProp, Value = value }});
            }}
            return list;
        }}

        private static List<string> ReadTextParts(byte[] bytes, ref int offset)
        {{
            int count = ReadU32(bytes, offset); offset += 4;
            var list = new List<string>(count);
            for (int i = 0; i < count; i++)
            {{
                byte partKind = bytes[offset++];
                var text = ReadString(bytes, ref offset);
                list.Add(partKind == 1 ? text : "{{" + text + "}}");
            }}
            return list;
        }}
    }}
}}
"#
    )
}

fn generate_macos_swiftui_runtime() -> String {
    generate_swiftui_runtime(SwiftUiProfile::Desktop)
}
#[cfg(test)]
#[path = "../../tests/path_wired/codegen_desktop_native_runtime.rs"]
mod codegen_desktop_native_runtime_tests;
