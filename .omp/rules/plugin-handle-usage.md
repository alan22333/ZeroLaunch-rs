---
description: PluginHandle 使用 — 插件必须通过 PluginHandle 访问平台能力，无对应方法时必须先添加到 PluginHandle
condition: "PluginHandle|HostApi::register"
scope: "tool:edit(src-tauri/src/**), tool:write(src-tauri/src/**), tool:edit(crates/plugin-api/src/**), tool:write(crates/plugin-api/src/**)"
---

# PluginHandle 使用

- **内置插件**（进程内）**必须** 通过 `PluginHandle`（从 `HostApi::register()` 获取）访问平台能力。可用方法列表见 `PluginHandle` 源码（`crates/plugin-api/src/host/plugin_handle.rs`）。**禁止** 内置插件绕过 handle 直接调用平台实现
- 如果某平台操作没有 `PluginHandle` 方法，**必须** 先添加到 `PluginHandle` 再使用（新增能力的三件套流程见 `add-platform-capability` 规则）
- 第三方插件（子进程）无法直接接触平台 API，经 SDK `host/*` RPC 访问宿主能力（见 `sdk-trace-module`/`third-party-plugin` 契约）——本规则的"经 PluginHandle"仅约束内置插件
