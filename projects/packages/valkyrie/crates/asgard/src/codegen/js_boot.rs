//! 最小浏览器启动脚本（无框架 runtime；逻辑在 WASM，JS 仅加载与绑定）。
//!
//! 页面 HTML 只允许 `<script src="boot.js">`；`start(manifest)` 由本文件自动执行，禁止内联业务 JS。

/// Manifest URL baked into auto-start (`manifest.json` or `/manifest.json`).
pub fn manifest_url_for_mode(relative: bool) -> &'static str {
    if relative { "manifest.json" } else { "/manifest.json" }
}

/// Markup-only script tag for boot.js (no inline code).
pub fn asgard_boot_script_tag(relative: bool) -> &'static str {
    if relative { r#"<script src="boot.js"></script>"# } else { r#"<script src="/boot.js"></script>"# }
}

/// Markup-only stylesheet link.
pub fn asgard_boot_stylesheet_tag(css_href: &str) -> String {
    format!(r#"<link rel="stylesheet" href="{css_href}">"#)
}

/// 生成 `boot.js`：加载 WASM glue、组件胶水，挂载 island，并在加载后自动 `start(manifest_url)`。
pub fn generate_boot_script(wasm_stem: &str, manifest_url: &str) -> String {
    let manifest_url = manifest_url.trim();
    format!(
        r#"(function (global) {{
  'use strict';

  var domHandles = [null];
  var wasmExports = null;
  var componentFactories = new Map();
  var signals = [];
  var signalSubscribers = [];
  var storeSubscribers = [];
  var batchDepth = 0;
  var pendingSignals = new Set();
  var eventBindings = new Map();

  function storeDomHandle(node) {{
    var id = domHandles.length;
    domHandles.push(node);
    return id;
  }}

  function getDomHandle(id) {{
    return domHandles[id] || null;
  }}

  function isLoaded() {{
    return wasmExports !== null;
  }}

  function callExport(name) {{
    if (!wasmExports || typeof wasmExports[name] !== 'function') {{
      throw new Error('WASM export not found: ' + name);
    }}
    return wasmExports[name]();
  }}

  function registerComponent(name, factory) {{
    componentFactories.set(name, factory);
  }}

  function parseDeps(csv, sigIds) {{
    if (!csv) return sigIds || [];
    if (csv === 'store') return ['store'];
    return csv.split(',').map(function (part) {{ return part.trim(); }}).filter(Boolean);
  }}

  function scheduleNotify(sigId) {{
    if (sigId === -2) {{
      if (batchDepth > 0) return;
      storeSubscribers.slice().forEach(function (fn) {{ fn(); }});
      return;
    }}
    if (batchDepth > 0) {{
      pendingSignals.add(sigId);
      return;
    }}
    runSignalSubscribers(sigId);
  }}

  function runSignalSubscribers(sigId) {{
    var subs = signalSubscribers[sigId];
    if (!subs) return;
    subs.slice().forEach(function (fn) {{ fn(); }});
  }}

  function flushBatch() {{
    pendingSignals.forEach(function (sigId) {{ runSignalSubscribers(sigId); }});
    pendingSignals.clear();
    storeSubscribers.slice().forEach(function (fn) {{ fn(); }});
  }}

  var hostApi = {{
    sigCreateI32: function (v) {{
      var id = signals.length;
      signals.push({{ kind: 'i32', value: v | 0 }});
      signalSubscribers[id] = [];
      return id;
    }},
    sigGetI32: function (id) {{ return (signals[id] && signals[id].value) || 0; }},
    sigSetI32: function (id, v) {{
      if (!signals[id]) return;
      signals[id].value = v | 0;
      scheduleNotify(id);
    }},
    sigCreateUtf8: function (v) {{
      var id = signals.length;
      signals.push({{ kind: 'utf8', value: String(v || '') }});
      signalSubscribers[id] = [];
      return id;
    }},
    sigGetUtf8: function (id) {{ return (signals[id] && signals[id].value) || ''; }},
    sigSetUtf8: function (id, v) {{
      if (!signals[id]) return;
      signals[id].value = String(v || '');
      scheduleNotify(id);
    }},
    sigCreateBool: function (v) {{
      var id = signals.length;
      signals.push({{ kind: 'bool', value: !!v }});
      signalSubscribers[id] = [];
      return id;
    }},
    sigGetBool: function (id) {{ return !!(signals[id] && signals[id].value); }},
    sigSetBool: function (id, v) {{
      if (!signals[id]) return;
      signals[id].value = !!v;
      scheduleNotify(id);
    }},
    sigCreateList: function () {{
      var id = signals.length;
      signals.push({{ kind: 'list', value: [] }});
      signalSubscribers[id] = [];
      return id;
    }},
    sigGetList: function () {{ return []; }},
    sigSetList: function (id) {{
      scheduleNotify(id);
    }},
    sigSubscribe: function (sigId, depsCsv, updateExport) {{
      var deps = parseDeps(depsCsv, [sigId]);
      var fn = function () {{
        if (wasmExports && typeof wasmExports[updateExport] === 'function') {{
          wasmExports[updateExport]();
        }}
      }};
      deps.forEach(function (dep) {{
        if (dep === 'store') {{
          storeSubscribers.push(fn);
          return;
        }}
        var depId = resolveSigRef(dep);
        if (depId >= 0) {{
          signalSubscribers[depId] = signalSubscribers[depId] || [];
          signalSubscribers[depId].push(fn);
        }}
      }});
      fn();
    }},
    rxBindTextUtf8: function (handle, exprExport, dep1, dep2) {{
      bindExpr(handle, exprExport, [dep1, dep2], function (node, value) {{
        node.textContent = value;
      }});
    }},
    rxBindAttrUtf8: function (handle, attr, exprExport, dep1, dep2) {{
      bindExpr(handle, exprExport, [dep1, dep2], function (node, value) {{
        node.setAttribute(attr, value);
      }});
    }},
    rxBindClassUtf8: function (handle, exprExport, dep1, dep2) {{
      bindExpr(handle, exprExport, [dep1, dep2], function (node, value) {{
        node.setAttribute('class', value);
      }});
    }},
    rxBindPropUtf8: function (sigId, exprExport, dep1, dep2) {{
      bindExpr(null, exprExport, [dep1, dep2], function (_node, value) {{
        hostApi.sigSetUtf8(sigId, value);
      }}, true);
    }},
    rxBindIf: function (container, condExport, mountExport, dep1, dep2) {{
      var mounted = false;
      var update = function () {{
        var show = wasmExports[condExport]();
        var node = getDomHandle(container);
        if (!node) return;
        if (show && !mounted) {{
          wasmExports[mountExport](container);
          mounted = true;
        }} else if (!show && mounted) {{
          node.textContent = '';
          mounted = false;
        }}
      }};
      subscribeDeps([dep1, dep2], update);
    }},
    rxBindLoop: function (container, itemsExport, mountExport, keyExport, dep1, dep2) {{
      var update = function () {{
        var node = getDomHandle(container);
        if (!node) return;
        while (node.firstChild) node.removeChild(node.firstChild);
        var items = wasmExports[itemsExport]() || [];
        for (var i = 0; i < items.length; i++) {{
          // Pass index so WASM loop_mount can bind `let item = items[index]`.
          wasmExports[mountExport](container, i);
        }}
      }};
      subscribeDeps([dep1, dep2], update);
      var _key = keyExport;
    }},
    rxMemoI32: function () {{ return 0; }},
    rxMemoUtf8: function () {{ return ''; }},
    domAddEventExport: function (handle, event, handlerExport) {{
      var node = getDomHandle(handle);
      if (!node) return;
      var fn = function () {{
        if (wasmExports && typeof wasmExports[handlerExport] === 'function') {{
          wasmExports[handlerExport]();
        }}
      }};
      node.addEventListener(event, fn);
      eventBindings.set(handlerExport, fn);
    }},
    domAddEventExportUtf8: function (handle, event, handlerExport, arg) {{
      var node = getDomHandle(handle);
      if (!node) return;
      var captured = arg;
      var fn = function () {{
        if (wasmExports && typeof wasmExports[handlerExport] === 'function') {{
          wasmExports[handlerExport](captured);
        }}
      }};
      node.addEventListener(event, fn);
      eventBindings.set(handlerExport + ':' + String(captured), fn);
    }},
    domAddEventExportI32: function (handle, event, handlerExport, arg) {{
      var node = getDomHandle(handle);
      if (!node) return;
      var captured = arg | 0;
      var fn = function () {{
        if (wasmExports && typeof wasmExports[handlerExport] === 'function') {{
          wasmExports[handlerExport](captured);
        }}
      }};
      node.addEventListener(event, fn);
      eventBindings.set(handlerExport + ':' + String(captured), fn);
    }},
    rxBatchBegin: function () {{ batchDepth += 1; }},
    rxBatchEnd: function () {{
      batchDepth = Math.max(0, batchDepth - 1);
      if (batchDepth === 0) flushBatch();
    }},
    storeBump: function () {{ scheduleNotify(-2); }},
    storeSubscribe: function (exportName) {{
      storeSubscribers.push(function () {{
        if (wasmExports && typeof wasmExports[exportName] === 'function') {{
          wasmExports[exportName]();
        }}
      }});
    }},
    styleCollectorPush: function (classes) {{
      var text = readUtf8(classes);
      if (!text) return;
      global.__voaStyleUtilities = global.__voaStyleUtilities || new Set();
      text.split(/\\s+/).forEach(function (token) {{
        if (token) global.__voaStyleUtilities.add(token);
      }});
    }}
  }};

  function readUtf8(ptr, len) {{
    if (typeof ptr === 'string') return ptr;
    return String(ptr || '');
  }}

  function resolveSigRef(name) {{
    if (typeof name === 'number') return name;
    if (name && name.endsWith && name.endsWith('_sig')) {{
      return -1;
    }}
    var parsed = parseInt(name, 10);
    return isNaN(parsed) ? -1 : parsed;
  }}

  function subscribeDeps(depIds, fn) {{
    var deps = (depIds || []).filter(function (dep) {{ return dep !== null && dep !== undefined && dep >= 0; }});
    if (depIds && depIds.indexOf(-2) >= 0) {{
      storeSubscribers.push(fn);
    }}
    if (deps.length === 0 && !(depIds && depIds.indexOf(-2) >= 0)) {{
      fn();
      return;
    }}
    deps.forEach(function (depId) {{
      signalSubscribers[depId] = signalSubscribers[depId] || [];
      signalSubscribers[depId].push(fn);
    }});
    fn();
  }}

  function bindExpr(handle, exprExport, depIds, apply, propMode) {{
    var update = function () {{
      var value = wasmExports[exprExport]();
      if (propMode) {{
        apply(null, value);
        return;
      }}
      var node = getDomHandle(handle);
      if (node) apply(node, value);
    }};
    subscribeDeps(depIds, update);
  }}

  function mountIslands() {{
    componentFactories.forEach(function (factory, name) {{
      var nodes = document.querySelectorAll('[data-component="' + name + '"]');
      nodes.forEach(function (host) {{
        if (host.__voaMounted) return;
        var node = factory(host);
        if (node && node.nodeType && node !== host) {{
          host.appendChild(node);
        }}
        host.__voaMounted = true;
      }});
    }});
  }}

  function loadScript(url) {{
    return new Promise(function (resolve, reject) {{
      var script = document.createElement('script');
      script.src = url;
      script.async = false;
      script.onload = function () {{ resolve(); }};
      script.onerror = function () {{ reject(new Error('script load failed: ' + url)); }};
      document.head.appendChild(script);
    }});
  }}

  async function start(manifestUrl) {{
    var manifest = await fetch(manifestUrl).then(function (r) {{
      if (!r.ok) throw new Error('manifest fetch failed: ' + r.status);
      return r.json();
    }});

    if (manifest.wasm && manifest.wasm.length > 0) {{
      var entry = manifest.wasm[0];
      var glue = await import(entry.glue);
      var imports = {{ __voa: hostApi, env: hostApi }};
      if (typeof glue.instantiate === 'function') {{
        var instance = await glue.instantiate(entry.url, imports);
        wasmExports = instance.exports;
      }} else if (typeof glue.run === 'function') {{
        var runInstance = await glue.run(entry.url, imports);
        wasmExports = runInstance.exports;
      }}
    }}

    if (manifest.components) {{
      for (var i = 0; i < manifest.components.length; i++) {{
        await loadScript(manifest.components[i].js);
      }}
    }}

    mountIslands();
    return manifest;
  }}

  function hydrateFromDom(manifestUrl) {{
    return start(manifestUrl);
  }}

  global.__voa = {{
    storeDomHandle: storeDomHandle,
    getDomHandle: getDomHandle,
    isLoaded: isLoaded,
    callExport: callExport,
    registerComponent: registerComponent,
    mountIslands: mountIslands,
    hydrate: hydrateFromDom,
    start: start,
    wasmModule: '{wasm_stem}',
    host: hostApi
  }};

  // Auto glue: page must not inline start(); boot self-starts on load.
  start('{manifest_url}').catch(function (e) {{
    console.error('asgard start failed:', e);
  }});
}})(typeof globalThis !== 'undefined' ? globalThis : window);
"#
    )
}
