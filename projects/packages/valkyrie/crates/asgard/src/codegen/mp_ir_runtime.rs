//! 微信小程序 `asgard-runtime.js`：UiHost ABI + WXWebAssembly；业务只经 wasm exports。

use crate::codegen::{UI_BIN_MAGIC, section_framing::magic_bytes_literal};

/// 生成薄 shim：加载 `.wasm`、解码 asgard ui、`on_event` → `awsl_call_*`、`syncFromWasm` → setData。
pub fn generate_mp_runtime(wasm_stem: &str) -> String {
    let magic = magic_bytes_literal(UI_BIN_MAGIC);
    format!("const WASM_MODULE = '{wasm_stem}';\nconst ASGARD_UI_MAGIC = [{magic}];\n{MP_RUNTIME_BODY}")
}

const MP_RUNTIME_BODY: &str = r#"
function readU32(view, offset) {
  return view.getUint32(offset, true);
}

function readString(view, offset) {
  const len = readU32(view, offset);
  const start = offset + 4;
  const bytes = new Uint8Array(view.buffer, view.byteOffset + start, len);
  return { value: new TextDecoder().decode(bytes), next: start + len };
}

function findAsgardUiSection(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const magicLen = ASGARD_UI_MAGIC.length;
  let last = null;
  let idx = 0;
  while (idx + magicLen + 4 <= bytes.byteLength) {
    let matched = true;
    for (let i = 0; i < magicLen; i++) {
      if (bytes[idx + i] !== ASGARD_UI_MAGIC[i]) {
        matched = false;
        break;
      }
    }
    if (!matched) {
      idx += 1;
      continue;
    }
    const len = readU32(view, idx + magicLen);
    const start = idx + magicLen + 4;
    const end = start + len;
    if (end > bytes.byteLength) break;
    last = bytes.subarray(start, end);
    idx = end;
  }
  return last;
}

function wasmModuleBytes(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const magicLen = ASGARD_UI_MAGIC.length;
  let lastStart = -1;
  let idx = 0;
  while (idx + magicLen + 4 <= bytes.byteLength) {
    let matched = true;
    for (let i = 0; i < magicLen; i++) {
      if (bytes[idx + i] !== ASGARD_UI_MAGIC[i]) {
        matched = false;
        break;
      }
    }
    if (!matched) {
      idx += 1;
      continue;
    }
    const len = readU32(view, idx + magicLen);
    const start = idx + magicLen + 4;
    const end = start + len;
    if (end > bytes.byteLength) break;
    lastStart = idx;
    idx = end;
  }
  if (lastStart >= 0) {
    return bytes.subarray(0, lastStart);
  }
  return bytes;
}

function readBindings(view, offset) {
  const count = readU32(view, offset);
  let cursor = offset + 4;
  const bindings = [];
  for (let i = 0; i < count; i++) {
    const name = readString(view, cursor);
    cursor = name.next;
    const init = readString(view, cursor);
    cursor = init.next;
    const reactive = view.getUint8(cursor) !== 0;
    const valueType = view.getUint8(cursor + 1);
    cursor += 2;
    bindings.push({ name: name.value, init: init.value, reactive, valueType });
  }
  return { bindings, next: cursor };
}

function readTextParts(view, offset) {
  const count = readU32(view, offset);
  let cursor = offset + 4;
  const parts = [];
  for (let i = 0; i < count; i++) {
    const kind = view.getUint8(cursor);
    cursor += 1;
    const text = readString(view, cursor);
    cursor = text.next;
    parts.push(kind === 1 ? { static: text.value } : { dynamic: text.value });
  }
  return { parts, next: cursor };
}

function readAttrs(view, offset) {
  const count = readU32(view, offset);
  let cursor = offset + 4;
  const attrs = [];
  for (let i = 0; i < count; i++) {
    const name = readString(view, cursor);
    cursor = name.next;
    const isEvent = view.getUint8(cursor) !== 0;
    const isProp = view.getUint8(cursor + 1) !== 0;
    const valueKind = view.getUint8(cursor + 2);
    cursor += 3;
    let value;
    if (valueKind === 1 || valueKind === 2) {
      const text = readString(view, cursor);
      cursor = text.next;
      value = text.value;
    } else {
      const mixed = readTextParts(view, cursor);
      cursor = mixed.next;
      value = mixed.parts;
    }
    attrs.push({ name: name.value, isEvent, isProp, valueKind, value });
  }
  return { attrs, next: cursor };
}

function readIr(view, offset) {
  const count = readU32(view, offset);
  let cursor = offset + 4;
  const nodes = [];
  for (let i = 0; i < count; i++) {
    const kind = view.getUint8(cursor);
    cursor += 1;
    if (kind === 1) {
      const tag = readString(view, cursor);
      cursor = tag.next;
      const nodeKind = view.getUint8(cursor);
      cursor += 1;
      const attrs = readAttrs(view, cursor);
      cursor = attrs.next;
      const children = readIr(view, cursor);
      cursor = children.next;
      nodes.push({ kind: 'tag', tag: tag.value, nodeKind, attrs: attrs.attrs, children: children.nodes });
    } else if (kind === 2) {
      const parts = readTextParts(view, cursor);
      cursor = parts.next;
      nodes.push({ kind: 'text', parts: parts.parts });
    } else if (kind === 3) {
      const cond = readString(view, cursor);
      cursor = cond.next;
      const thenBranch = readIr(view, cursor);
      cursor = thenBranch.next;
      const elseBranch = readIr(view, cursor);
      cursor = elseBranch.next;
      nodes.push({ kind: 'if', cond: cond.value, then: thenBranch.nodes, else: elseBranch.nodes });
    } else if (kind === 4) {
      const items = readString(view, cursor);
      cursor = items.next;
      const itemVar = readString(view, cursor);
      cursor = itemVar.next;
      const body = readIr(view, cursor);
      cursor = body.next;
      nodes.push({ kind: 'loop', items: items.value, itemVar: itemVar.value, body: body.nodes });
    }
  }
  return { nodes, next: cursor };
}

function decodeAsgardUi(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  for (let i = 0; i < ASGARD_UI_MAGIC.length; i++) {
    if (bytes[i] !== ASGARD_UI_MAGIC[i]) {
      throw new Error('invalid Asgard UI wire magic');
    }
  }
  let offset = ASGARD_UI_MAGIC.length;
  let version = 1;
  if (offset < bytes.byteLength && bytes[offset] === 0x02) {
    version = 0x02;
    offset += 1;
  }
  const componentCount = readU32(view, offset);
  offset += 4;
  const components = [];
  for (let i = 0; i < componentCount; i++) {
    const route = readString(view, offset);
    offset = route.next;
    const name = readString(view, offset);
    offset = name.next;
    if (version === 0x02) {
      offset = skipAbi(view, offset);
    }
    const bindings = readBindings(view, offset);
    offset = bindings.next;
    const ir = readIr(view, offset);
    offset = ir.next;
    components.push({
      route: route.value,
      name: name.value,
      bindings: bindings.bindings,
      ir: ir.nodes
    });
  }
  return components;
}

function skipAbi(view, offset) {
  let count = readU32(view, offset);
  offset += 4;
  for (let i = 0; i < count; i++) {
    const prop = readString(view, offset);
    offset = prop.next;
    offset += 1;
    const flags = view.getUint8(offset);
    offset += 1;
    if (flags & 2) {
      const def = readString(view, offset);
      offset = def.next;
    }
  }
  count = readU32(view, offset);
  offset += 4;
  for (let i = 0; i < count; i++) {
    const event = readString(view, offset);
    offset = event.next;
    const paramCount = readU32(view, offset);
    offset += 4;
    for (let j = 0; j < paramCount; j++) {
      const param = readString(view, offset);
      offset = param.next;
      offset += 1;
    }
  }
  return offset;
}

function sanitizeIdent(name) {
  return String(name || '').replace(/[^A-Za-z0-9_]/g, '_').replace(/^_+|_+$/g, '').toLowerCase();
}

function resolveCallExport(name) {
  const trimmed = String(name || '').trim();
  if (trimmed.indexOf('awsl_call_') === 0) return trimmed;
  return 'awsl_call_' + trimmed;
}

function resolveSigExport(route, bindingName) {
  return 'awsl_sig_' + sanitizeIdent(route) + '_' + sanitizeIdent(bindingName);
}

function parseInitValue(init, valueType) {
  const trimmed = (init || '').trim();
  if (valueType === 3) return trimmed === 'true';
  if (valueType === 1) {
    const n = Number.parseInt(trimmed, 10);
    return Number.isNaN(n) ? 0 : n;
  }
  return trimmed;
}

function bindingsToData(bindings) {
  const data = {};
  for (const binding of bindings) {
    if (!binding.reactive) continue;
    data[binding.name] = parseInitValue(binding.init, binding.valueType);
  }
  return data;
}

var signals = [];
var signalSubscribers = [];
var storeSubscribers = [];
var batchDepth = 0;
var pendingSignals = new Set();
var wasmExports = null;

function parseDeps(csv, sigIds) {
  if (!csv) return sigIds || [];
  if (csv === 'store') return ['store'];
  return csv.split(',').map(function (part) { return part.trim(); }).filter(Boolean);
}

function resolveSigRef(name) {
  if (typeof name === 'number') return name;
  if (name && name.endsWith && name.endsWith('_sig')) return -1;
  var parsed = parseInt(name, 10);
  return isNaN(parsed) ? -1 : parsed;
}

function scheduleNotify(sigId) {
  if (sigId === -2) {
    if (batchDepth > 0) return;
    storeSubscribers.slice().forEach(function (fn) { fn(); });
    return;
  }
  if (batchDepth > 0) {
    pendingSignals.add(sigId);
    return;
  }
  var subs = signalSubscribers[sigId];
  if (!subs) return;
  subs.slice().forEach(function (fn) { fn(); });
}

function flushBatch() {
  pendingSignals.forEach(function (sigId) {
    var subs = signalSubscribers[sigId];
    if (!subs) return;
    subs.slice().forEach(function (fn) { fn(); });
  });
  pendingSignals.clear();
  storeSubscribers.slice().forEach(function (fn) { fn(); });
}

var hostApi = {
  sigCreateI32: function (v) {
    var id = signals.length;
    signals.push({ kind: 'i32', value: v | 0 });
    signalSubscribers[id] = [];
    return id;
  },
  sigGetI32: function (id) { return (signals[id] && signals[id].value) || 0; },
  sigSetI32: function (id, v) {
    if (!signals[id]) return;
    signals[id].value = v | 0;
    scheduleNotify(id);
  },
  sigCreateUtf8: function (v) {
    var id = signals.length;
    signals.push({ kind: 'utf8', value: String(v || '') });
    signalSubscribers[id] = [];
    return id;
  },
  sigGetUtf8: function (id) { return (signals[id] && signals[id].value) || ''; },
  sigSetUtf8: function (id, v) {
    if (!signals[id]) return;
    signals[id].value = String(v || '');
    scheduleNotify(id);
  },
  sigCreateBool: function (v) {
    var id = signals.length;
    signals.push({ kind: 'bool', value: !!v });
    signalSubscribers[id] = [];
    return id;
  },
  sigGetBool: function (id) { return !!(signals[id] && signals[id].value); },
  sigSetBool: function (id, v) {
    if (!signals[id]) return;
    signals[id].value = !!v;
    scheduleNotify(id);
  },
  sigCreateList: function () {
    var id = signals.length;
    signals.push({ kind: 'list', value: [] });
    signalSubscribers[id] = [];
    return id;
  },
  sigGetList: function () { return []; },
  sigSetList: function (id) { scheduleNotify(id); },
  sigSubscribe: function (sigId, depsCsv, updateExport) {
    var deps = parseDeps(depsCsv, [sigId]);
    var fn = function () {
      if (wasmExports && typeof wasmExports[updateExport] === 'function') {
        wasmExports[updateExport]();
      }
      voaRuntime.syncFromWasm();
    };
    deps.forEach(function (dep) {
      if (dep === 'store') {
        storeSubscribers.push(fn);
        return;
      }
      var depId = resolveSigRef(dep);
      if (depId >= 0) {
        signalSubscribers[depId] = signalSubscribers[depId] || [];
        signalSubscribers[depId].push(fn);
      }
    });
    fn();
  },
  rxBatchBegin: function () { batchDepth += 1; },
  rxBatchEnd: function () {
    batchDepth -= 1;
    if (batchDepth <= 0) {
      batchDepth = 0;
      flushBatch();
    }
  },
  storeBump: function () { scheduleNotify(-2); },
  storeSubscribe: function (exportName) {
    storeSubscribers.push(function () {
      if (wasmExports && typeof wasmExports[exportName] === 'function') {
        wasmExports[exportName]();
      }
      voaRuntime.syncFromWasm();
    });
  }
};

function readWasmBytes(path) {
  const fs = wx.getFileSystemManager();
  const candidates = [path, '/' + WASM_MODULE + '.wasm', WASM_MODULE + '.wasm'];
  let lastError = null;
  for (const candidate of candidates) {
    try {
      const buf = fs.readFileSync(candidate);
      return new Uint8Array(buf);
    } catch (error) {
      lastError = error;
    }
  }
  throw lastError || new Error('failed to read wasm: ' + WASM_MODULE);
}

const voaRuntime = {
  host: null,
  exports: null,
  components: [],
  page: null,
  component: null,

  mount(component) {
    this.component = component;
    if (!this.page || !component) return;
    this.page.setData(bindingsToData(component.bindings));
    this.syncFromWasm();
  },

  patch(key, value) {
    if (!this.page) return;
    const data = {};
    data[key] = value;
    this.page.setData(data);
  },

  on_event(name) {
    return this.callExport(resolveCallExport(name));
  },

  syncFromWasm() {
    if (!this.page || !this.component || !this.exports) return;
    const patch = {};
    for (const binding of this.component.bindings) {
      if (!binding.reactive) continue;
      const exportName = resolveSigExport(this.component.route, binding.name);
      const fn = this.exports[exportName];
      if (typeof fn !== 'function') continue;
      patch[binding.name] = fn();
    }
    if (Object.keys(patch).length > 0) {
      this.page.setData(patch);
    }
  },

  async loadProduct() {
    const path = '/' + WASM_MODULE + '.wasm';
    const bytes = readWasmBytes(path);
    this.host = bytes;
    const section = findAsgardUiSection(bytes);
    if (section) {
      this.components = decodeAsgardUi(section);
    }
    if (typeof WXWebAssembly === 'undefined') {
      throw new Error('WXWebAssembly unavailable');
    }
    const moduleBytes = wasmModuleBytes(bytes);
    const imports = { __voa: hostApi, env: hostApi };
    const result = await WXWebAssembly.instantiate(
      moduleBytes.buffer.slice(moduleBytes.byteOffset, moduleBytes.byteOffset + moduleBytes.byteLength),
      imports
    );
    this.exports = result.instance.exports;
    wasmExports = this.exports;
    return this.host;
  },

  async loadHost(_stem) {
    return this.loadProduct();
  },

  callExport(name, ...args) {
    const exportName = resolveCallExport(name);
    if (!this.exports || typeof this.exports[exportName] !== 'function') {
      throw new Error('WASM export not found: ' + exportName);
    }
    const ret = this.exports[exportName](...args);
    this.syncFromWasm();
    return ret;
  },

  bindPage(page, hydrateExport) {
    this.page = page;
    page.__voaHydrate = hydrateExport;
    const routeKey = String(hydrateExport || '').replace(/^awsl_hydrate_/, '');
    const component = this.components.find((c) => sanitizeIdent(c.route) === sanitizeIdent(routeKey))
      || this.components[0];
    if (component) {
      const initName = 'awsl_mp_init_' + sanitizeIdent(component.route);
      if (typeof this.exports[initName] === 'function') {
        this.exports[initName]();
      }
      this.mount(component);
    }
  }
};

module.exports = voaRuntime;
"#;
#[cfg(test)]
#[path = "../../tests/path_wired/codegen_mp_ir_runtime.rs"]
mod codegen_mp_ir_runtime_tests;
