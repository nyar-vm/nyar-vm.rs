//! Android Compose `AsgardComposeRuntime.kt`：asgard ui wire + ASGARDNT `System.load` + UiHost ABI。

use crate::codegen::{HOST_NATIVE_MAGIC, UI_BIN_MAGIC, magic_bytes_literal};

/// 生成含 Asgard UI 解码器的 Compose 运行时（构建期模板；Release 不交付 `.kt`）。
pub fn generate_android_compose_runtime() -> String {
    let ui_magic = magic_bytes_literal(UI_BIN_MAGIC);
    let native_magic = magic_bytes_literal(HOST_NATIVE_MAGIC);
    r#"package com.asgard.runtime

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import java.nio.ByteBuffer
import java.nio.ByteOrder

private val ASGARD_UI_MAGIC = byteArrayOf(__UI_MAGIC__)
private val ASGARD_NATIVE_MAGIC = byteArrayOf(__NATIVE_MAGIC__)

data class AsgardBinding(val name: String, val init: String, val reactive: Boolean, val valueType: Int)
data class AsgardAttr(val name: String, val isEvent: Boolean, val isProp: Boolean, val value: String)
data class AsgardNode(
    val kind: Int,
    val tag: String = "",
    val attrs: List<AsgardAttr> = emptyList(),
    val children: List<AsgardNode> = emptyList(),
    val textParts: List<String> = emptyList(),
    val cond: String = "",
    val loopItems: String = "",
    val loopItemVar: String = ""
)
data class AsgardComponent(
    val route: String,
    val name: String,
    val bindings: List<AsgardBinding>,
    val nodes: List<AsgardNode>
)

object AsgardComposeRuntime {
    private var components: List<AsgardComponent> = emptyList()
    private val reactive = mutableStateMapOf<String, String>()

    fun loadFromProduct(product: ByteArray, logicFile: java.io.File) {
        AsgardHostBridge.loadNativeFromProduct(product, logicFile)
        val section = findAsgardUiSection(product) ?: return
        components = decodeAsgardUi(section)
        mountAll()
    }

    @Deprecated("Use loadFromProduct", ReplaceWith("loadFromProduct(product, logicFile)"))
    fun loadFromHostImage(hostImage: ByteArray, logicFile: java.io.File) = loadFromProduct(hostImage, logicFile)

    @Deprecated("Use loadFromProduct", ReplaceWith("loadFromProduct(dex, logicFile)"))
    fun loadFromDex(dex: ByteArray, logicFile: java.io.File) = loadFromProduct(dex, logicFile)

    private fun resolveCallExport(name: String): String {
        val trimmed = name.trim()
        return if (trimmed.startsWith("awsl_call_")) trimmed else "awsl_call_$trimmed"
    }

    private fun collectEventNames(node: AsgardNode): List<String> {
        val events = node.attrs.filter { it.isEvent }.map { it.value }
        return events + node.children.flatMap { collectEventNames(it) }
    }

    fun mount(component: AsgardComponent) {
        component.bindings.filter { it.reactive }.forEach { reactive[it.name] = it.init.trim() }
    }

    fun patch(key: String, value: String) {
        reactive[key.trim()] = value
    }

    /** UiHost.on_event：只走 AOT export，无本地业务。 */
    fun on_event(name: String) {
        AsgardHostBridge.invokeExport(resolveCallExport(name))
    }

    fun dispatchEvent(name: String) {
        on_event(name)
    }

    @Composable
    fun AsgardRoot() {
        MaterialTheme {
            Surface(modifier = Modifier.fillMaxSize()) {
                Column(modifier = Modifier.padding(16.dp)) {
                    components.forEach { component ->
                        RenderNodes(component.nodes, reactive)
                    }
                }
            }
        }
    }

    @Composable
    private fun RenderNodes(nodes: List<AsgardNode>, state: Map<String, String>) {
        nodes.forEach { node -> RenderNode(node, state) }
    }

    @Composable
    private fun RenderNode(node: AsgardNode, state: Map<String, String>) {
        when (node.kind) {
            1 -> when (node.tag) {
                "Column" -> Column { node.children.forEach { RenderNode(it, state) } }
                "Row" -> Row { node.children.forEach { RenderNode(it, state) } }
                "Surface" -> Surface { node.children.forEach { RenderNode(it, state) } }
                "Text" -> Text(resolveText(node.textParts, state))
                "Button" -> {
                    val label = node.children.firstOrNull()?.textParts?.joinToString("") ?: "Button"
                    val event = node.attrs.firstOrNull { it.isEvent }?.value ?: ""
                    Button(onClick = { if (event.isNotEmpty()) dispatchEvent(event) }) {
                        Text(label)
                    }
                }
                else -> Text(node.tag)
            }
            2 -> Text(resolveText(node.textParts, state))
            3 -> if (evalCond(node.cond, state)) {
                RenderNodes(node.children, state)
            }
            4 -> {
                val count = evalLoopCount(node.loopItems, state)
                repeat(count.coerceAtLeast(0)) { idx ->
                    val loopState = state.toMutableMap()
                    if (node.loopItemVar.isNotEmpty()) {
                        loopState[node.loopItemVar] = idx.toString()
                    }
                    RenderNodes(node.children, loopState)
                }
            }
            else -> Unit
        }
    }

    private fun evalLoopCount(expr: String, state: Map<String, String>): Int {
        val key = expr.trim()
        state[key]?.toIntOrNull()?.let { return it }
        return key.toIntOrNull() ?: 1
    }

    private fun evalCond(cond: String, state: Map<String, String>): Boolean {
        val key = cond.trim()
        return state[key]?.toBooleanStrictOrNull() ?: (key == "true" || key.toIntOrNull()?.let { it != 0 } == true)
    }

    private fun resolveText(parts: List<String>, state: Map<String, String>): String {
        return parts.joinToString("") { part ->
            if (part.startsWith("{") && part.endsWith("}")) {
                val key = part.substring(1, part.length - 1)
                state[key] ?: part
            } else part
        }
    }

    private fun mountAll() {
        components.forEach { mount(it) }
    }

    private fun findSection(bytes: ByteArray, magic: ByteArray): ByteArray? {
        val magicLen = magic.size
        var idx = 0
        var last: ByteArray? = null
        val view = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        while (idx + magicLen + 4 <= bytes.size) {
            var matched = true
            for (i in 0 until magicLen) {
                if (bytes[idx + i] != magic[i]) { matched = false; break }
            }
            if (!matched) { idx++; continue }
            view.position(idx + magicLen)
            val len = view.int
            val start = idx + magicLen + 4
            val end = start + len
            if (end > bytes.size) break
            last = bytes.copyOfRange(start, end)
            idx = end
        }
        return last
    }

    private fun findAsgardUiSection(bytes: ByteArray): ByteArray? = findSection(bytes, ASGARD_UI_MAGIC)

    fun findAsgardNativeSection(bytes: ByteArray): ByteArray? = findSection(bytes, ASGARD_NATIVE_MAGIC)

    private fun decodeAsgardUi(bytes: ByteArray): List<AsgardComponent> {
        val view = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        for (i in ASGARD_UI_MAGIC.indices) {
            require(bytes[i] == ASGARD_UI_MAGIC[i]) { "invalid Asgard UI wire magic" }
        }
        var offset = ASGARD_UI_MAGIC.size
        var version = 1
        if (offset < bytes.size && bytes[offset] == 0x02.toByte()) {
            version = 0x02
            offset += 1
        }
        view.position(offset)
        val count = view.int
        offset += 4
        val out = mutableListOf<AsgardComponent>()
        repeat(count) {
            val route = readString(bytes, offset); offset = route.second
            val name = readString(bytes, offset); offset = name.second
            if (version == 0x02) {
                offset = skipAbi(bytes, offset)
            }
            val bindings = readBindings(bytes, offset); offset = bindings.second
            val nodes = readIr(bytes, offset); offset = nodes.second
            out += AsgardComponent(route.first, name.first, bindings.first, nodes.first)
        }
        return out
    }

    private fun skipAbi(bytes: ByteArray, offset: Int): Int {
        var cursor = offset
        val view = ByteBuffer.wrap(bytes, cursor, bytes.size - cursor).order(ByteOrder.LITTLE_ENDIAN)
        var count = view.int
        cursor += 4
        repeat(count) {
            val prop = readString(bytes, cursor); cursor = prop.second
            cursor += 1
            val flags = bytes[cursor]; cursor += 1
            if (flags.toInt() and 2 != 0) {
                val def = readString(bytes, cursor); cursor = def.second
            }
        }
        val view2 = ByteBuffer.wrap(bytes, cursor, bytes.size - cursor).order(ByteOrder.LITTLE_ENDIAN)
        count = view2.int
        cursor += 4
        repeat(count) {
            val event = readString(bytes, cursor); cursor = event.second
            val paramCount = ByteBuffer.wrap(bytes, cursor, bytes.size - cursor).order(ByteOrder.LITTLE_ENDIAN).int
            cursor += 4
            repeat(paramCount) {
                val param = readString(bytes, cursor); cursor = param.second
                cursor += 1
            }
        }
        return cursor
    }

    private fun readString(bytes: ByteArray, offset: Int): Pair<String, Int> {
        val view = ByteBuffer.wrap(bytes, offset, bytes.size - offset).order(ByteOrder.LITTLE_ENDIAN)
        val len = view.int
        val start = offset + 4
        return Pair(String(bytes, start, len, Charsets.UTF_8), start + len)
    }

    private fun readBindings(bytes: ByteArray, offset: Int): Pair<List<AsgardBinding>, Int> {
        val view = ByteBuffer.wrap(bytes, offset, bytes.size - offset).order(ByteOrder.LITTLE_ENDIAN)
        val count = view.int
        var cursor = offset + 4
        val out = mutableListOf<AsgardBinding>()
        repeat(count) {
            val name = readString(bytes, cursor); cursor = name.second
            val init = readString(bytes, cursor); cursor = init.second
            val reactive = bytes[cursor] != 0.toByte()
            val valueType = bytes[cursor + 1].toInt()
            cursor += 2
            out += AsgardBinding(name.first, init.first, reactive, valueType)
        }
        return Pair(out, cursor)
    }

    private fun readIr(bytes: ByteArray, offset: Int): Pair<List<AsgardNode>, Int> {
        val view = ByteBuffer.wrap(bytes, offset, bytes.size - offset).order(ByteOrder.LITTLE_ENDIAN)
        val count = view.int
        var cursor = offset + 4
        val out = mutableListOf<AsgardNode>()
        repeat(count) {
            val kind = bytes[cursor].toInt(); cursor++
            when (kind) {
                1 -> {
                    val tag = readString(bytes, cursor); cursor = tag.second
                    val nodeKind = bytes[cursor].toInt(); cursor++
                    val attrs = readAttrs(bytes, cursor); cursor = attrs.second
                    val children = readIr(bytes, cursor); cursor = children.second
                    out += AsgardNode(kind, tag.first, attrs.first, children.first)
                }
                2 -> {
                    val parts = readTextParts(bytes, cursor); cursor = parts.second
                    out += AsgardNode(kind, textParts = parts.first)
                }
                3 -> {
                    val cond = readString(bytes, cursor); cursor = cond.second
                    val thenBranch = readIr(bytes, cursor); cursor = thenBranch.second
                    val elseBranch = readIr(bytes, cursor); cursor = elseBranch.second
                    out += AsgardNode(kind, cond = cond.first, children = thenBranch.first + elseBranch.first)
                }
                4 -> {
                    val items = readString(bytes, cursor); cursor = items.second
                    val itemVar = readString(bytes, cursor); cursor = itemVar.second
                    val body = readIr(bytes, cursor); cursor = body.second
                    out += AsgardNode(kind, loopItems = items.first, loopItemVar = itemVar.first, children = body.first)
                }
            }
        }
        return Pair(out, cursor)
    }

    private fun readAttrs(bytes: ByteArray, offset: Int): Pair<List<AsgardAttr>, Int> {
        val view = ByteBuffer.wrap(bytes, offset, bytes.size - offset).order(ByteOrder.LITTLE_ENDIAN)
        val count = view.int
        var cursor = offset + 4
        val out = mutableListOf<AsgardAttr>()
        repeat(count) {
            val name = readString(bytes, cursor); cursor = name.second
            val isEvent = bytes[cursor] != 0.toByte()
            val isProp = bytes[cursor + 1] != 0.toByte()
            val valueKind = bytes[cursor + 2].toInt()
            cursor += 3
            val value = when (valueKind) {
                1, 2 -> readString(bytes, cursor).also { cursor = it.second }.first
                else -> readTextParts(bytes, cursor).also { cursor = it.second }.first.joinToString("")
            }
            out += AsgardAttr(name.first, isEvent, isProp, value)
        }
        return Pair(out, cursor)
    }

    private fun readTextParts(bytes: ByteArray, offset: Int): Pair<List<String>, Int> {
        val view = ByteBuffer.wrap(bytes, offset, bytes.size - offset).order(ByteOrder.LITTLE_ENDIAN)
        val count = view.int
        var cursor = offset + 4
        val out = mutableListOf<String>()
        repeat(count) {
            val kind = bytes[cursor].toInt(); cursor++
            val text = readString(bytes, cursor); cursor = text.second
            out += if (kind == 1) text.first else "{${text.first}}"
        }
        return Pair(out, cursor)
    }
}

class MainActivity : androidx.activity.ComponentActivity() {
    override fun onCreate(savedInstanceState: android.os.Bundle?) {
        super.onCreate(savedInstanceState)
        val hostImage = runCatching {
            assets.open("asgard/classes.dex").readBytes()
        }.getOrElse {
            java.io.File(applicationInfo.sourceDir).readBytes()
        }
        val logicFile = java.io.File(codeCacheDir, "asgard_logic.so")
        AsgardComposeRuntime.loadFromProduct(hostImage, logicFile)
        setContent { AsgardComposeRuntime.AsgardRoot() }
    }
}

object AsgardHostBridge {
    /** 从制品 ASGARDNT 段抽出 .so → codeCacheDir → System.load。
     *  ASGARDNT .so 自包含 JNI_OnLoad 与 `asgard_invoke_export`（无外部 C/NDK）。 */
    fun loadNativeFromProduct(product: ByteArray, outFile: java.io.File) {
        val native = AsgardComposeRuntime.findAsgardNativeSection(product)
            ?: error("product missing ASGARDNT section (.so host logic required)")
        outFile.outputStream().use { it.write(native) }
        System.load(outFile.absolutePath)
    }

    /** JNI native — 由 ASGARDNT .so 的 JNI_OnLoad / RegisterNatives 实现。 */
    @JvmStatic
    external fun invokeExport(name: String)

    /** AOT 经 `asgard_patch_native` 回调，驱动 Compose reactive 更新。 */
    @JvmStatic
    fun patchFromNative(key: String, value: String) {
        AsgardComposeRuntime.patch(key, value)
    }
}
"#
    .replace("__UI_MAGIC__", &ui_magic)
    .replace("__NATIVE_MAGIC__", &native_magic)
}
#[cfg(test)]
#[path = "../../tests/path_wired/codegen_android_compose_runtime.rs"]
mod codegen_android_compose_runtime_tests;
