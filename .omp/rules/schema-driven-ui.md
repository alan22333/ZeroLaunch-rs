---
description: Schema 驱动设置 UI — 后端 SettingDefinition 定义 → 前端通用渲染，禁止为特定组件创建专用设置页
condition: "SettingDefinition|SchemaBuilder|DynamicFormField|SchemaKind|WidgetHint|FieldUiMetadata|ArrayUiKind"
scope: "tool:edit(crates/plugin-api/src/config/**), tool:write(crates/plugin-api/src/config/**), tool:edit(src-tauri/src/builtin_plugin/config/**), tool:write(src-tauri/src/builtin_plugin/config/**), tool:edit(src-ui/**/*.vue), tool:edit(src-ui/**/*.ts), tool:write(src-ui/**/*.vue), tool:write(src-ui/**/*.ts)"
---

# Schema 驱动的设置 UI

- 后端定义 `SettingDefinition`（通过 SchemaBuilder）→ 前端通用渲染
- **禁止** 为特定组件创建专用 Vue 设置页面（除非该组件有 `DetailPreviewPanel` 类扩展需求）
- `DynamicFormField.vue` 是唯一的字段渲染分发器。新增字段类型 → 在此添加分支
- 新增 `SchemaKind` 变体（Rust 侧 `crates/plugin-api/src/config`）或 `WidgetHint`/`FieldUiMetadata` 变体 → 前端同步更新 `bridge/contract.ts` + `utils/schemaTypes.ts`；`ArrayUiKind`（前端 `schemaTypes.ts`）与 Rust `WidgetHint::Array` 系列对应
