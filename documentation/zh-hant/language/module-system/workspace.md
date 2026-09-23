# Workspace 模式

## 概述

Workspace 模式是 Valkyrie 語言的多項目管理模式，透過 `legions.json` 文件定義工作區配置。當項目根目錄存在 `legions.json` 文件時，該目錄被識別為 workspace 模式。

## 文件結構

```
workspace-root/
├── legions.json          # Workspace 配置文件
├── project-a/
│   ├── legion.json       # 項目 A 配置
│   ├── library/
│   │   └── _.vk         # 建議：命名空間根目錄定義
│   └── binary/
│       └── main.vk      # 註冊為指令 `main`
├── project-b/
│   ├── legion.json       # 項目 B 配置
│   ├── library/
│   │   └── _.vk         # 庫代碼目錄
│   └── binary/
│       ├── tool1.vk     # 註冊為指令 `tool1`
│       └── tool2/
│           └── _.vk     # 註冊為指令 `tool2`
└── shared/
    └── common/
        └── _.vk
```

## legions.json 配置

`legions.json` 是 JSON5 格式的配置文件，支持注釋和更靈活的語法：

```json5
{
    // Workspace 基本信息
    "name": "valkyrie-workspace",
    "version": "1.0.0",
    "description": "Valkyrie language workspace",
    
    // 私有工作區標識
    "private": true,
    
    // 成員項目列表
    "members": [
        "projects/*",
        "tools/build-tools"
    ],
    
    // 排除的目錄
    "exclude": [
        "legacy/*",
        "experiments/*",
        "temp/*"
    ],
    
    // 默認成員（用於快速構建）
    "default-members": [
        "projects/valkyrie-core",
        "projects/valkyrie-std"
    ],
    
    // Workspace 級別的腳本
    "scripts": {
        "build": "legion build --release",
        "test": "legion test --release",
        "fmt": "legion fmt --all",
        "clean": "legion clean",
        "publish": "git push && git push --tags --prune",
        "upgrade": "legion upgrade --workspace"
    },
    
    // 共享依賴配置
    "dependencies": {
        "shared": {
            "serde": "^1.0",
            "tokio": "^1.0"
        }
    },
    
    // 構建配置
    "build": {
        "profile": {
            "release": {
                "lto": true,
                "opt-level": "s"
            }
        }
    }
}
```

## 語義特性

### 1. 項目發現

- **自動發現**：根據 `members` 模式自動發現子項目
- **顯式排除**：透過 `exclude` 排除不需要的目錄
