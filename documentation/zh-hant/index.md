---
layout: home

hero:
  name: "Valkyrie"
  text: "現代函數式編程語言"
  tagline: 融合代數效應與強類型系統的下一代編程語言
  image:
    src: /logo.svg
    alt: Valkyrie
  actions:
    - theme: brand
      text: 快速開始
      link: /guide/getting-started
    - theme: alt
      text: 查看示例
      link: /examples/

features:
  - icon: 🎭
    title: 代數效應系統
    details: 原生支持代數效應，優雅處理異常、異步、狀態管理等副作用，提供比傳統異常處理更強大和靈活的控制流抽象。
  - icon: 🔒
    title: 強類型系統
    details: 先進的類型系統支持泛型、類型推導、模式匹配，在編譯時捕獲更多錯誤，提供卓越的開發體驗和代碼安全性。
  - icon: 🚀
    title: 多目標編譯
    details: 編譯到WebAssembly、JavaScript和原生代碼，一套代碼運行在瀏覽器、服務器和桌面環境，真正實現"一次編寫，到處運行"。
  - icon: ⚡️
    title: 函數式編程
    details: 不可變數據結構、高階函數、尾調用優化等函數式特性，結合現代語法設計，讓代碼更簡潔、更可靠、更易維護。
  - icon: 🛠️
    title: 現代工具鏈
    details: 集成的包管理器、格式化工具、語言服務器，提供完整的開發生態系統，支持增量編譯和智能代碼補全。
  - icon: 🌐
    title: 漸進式採用
    details: 可與現有JavaScript/TypeScript項目無縫集成，支持漸進式遷移，降低學習成本和遷移風險。
---

## 什麼是 Valkyrie？

Valkyrie 是一門現代的函數式編程語言，專為構建可靠、高性能的應用程序而設計。它將代數效應系統與強類型系統相結合，為開發者提供了一種全新的編程體驗。

### 核心特性

- **代數效應**: 優雅處理副作用，統一異常、異步、狀態管理
- **強類型系統**: 編譯時錯誤檢查，支持類型推導和模式匹配
- **多目標編譯**: 編譯到WebAssembly、JavaScript和原生代碼
- **函數式編程**: 不可變數據、高階函數、尾調用優化
- **現代語法**: 簡潔表達力強的語法設計
- **漸進式採用**: 與現有生態系統無縫集成

### 快速示例

```valkyrie
// 定義代數效應
effect Http {
    get(url: String): String
    post(url: String, body: String): String
}

// 使用效應的函數
micro fetch_user_data(id: Int) -> User {
    let response = raise Http::get(`/api/users/{id}`)
    parse_json(response)
}

// 效應處理器
micro main() {
    try {
        fetch_user_data(42)
    }
    .catch {
        case Http::get(url): resume(http_client.get(url))
        case Http::post(url, body): resume(http_client.post(url, body))
    }
}

// 模式匹配和類型安全
match user_result {
    Some(u) if u.age >= 18: print("成年用戶: {u.name}"),
    Some(u): print("未成年用戶: {u.name}"),
    None: print("用戶不存在")
}
```

這個簡單的 Valkyrie 程序展示了：
- 代數效應的定義和使用
- 強類型系統和類型推導
- 模式匹配和條件守衛
- 字符串插值和現代語法

## 開始使用

準備好體驗 Valkyrie 的強大功能了嗎？

[快速開始 →](/guide/getting-started)

## 為什麼選擇 Valkyrie？

### 🎯 **解決真實問題**
傳統編程語言在處理副作用時往往力不從心。Valkyrie 的代數效應系統提供了一種優雅的解決方案，讓異常處理、異步編程、狀態管理變得簡單而強大。
