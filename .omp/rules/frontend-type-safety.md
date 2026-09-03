---
description: 前端类型安全 — IPC 类型在 contract.ts 定义，类型守卫集中在 schemaTypes.ts
condition: "contract\\.ts|schemaTypes|isMyType"
scope: "tool:edit(*.ts), tool:edit(*.vue), tool:write(*.ts), tool:write(*.vue)"
---

# 前端类型安全

- 所有 IPC 类型在 `bridge/contract.ts` 中定义，与 Rust 字段级 `#[serde(rename = "camelCaseKey")]` 保持同步
- Schema 类型守卫集中在 `utils/schemaTypes.ts`。**禁止** 在组件中内联类型判断
- 边界说明：禁令针对**用户数据/IPC 载荷**的类型守卫（如判断某字段是否为 Record/数组）；对 `schema.type === 'array'` 这类 **schema 结构判别**（渲染分支）不受限——那是 UI 渲染逻辑，不是数据守卫

