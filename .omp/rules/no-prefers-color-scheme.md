---
description: 禁止使用 prefers-color-scheme 媒体查询 — 系统主题检测由后端驱动（bridge_get_system_theme + system-theme-changed 事件）
condition: "prefers-color-scheme"
scope: "tool:edit(*.css), tool:edit(*.vue), tool:edit(*.ts), tool:write(*.css), tool:write(*.vue), tool:write(*.ts)"
---

你使用了 `prefers-color-scheme`（CSS 媒体查询或 JS `window.matchMedia`）。系统主题检测已由后端统一提供，前端禁止直接检测：

- **初始值**：`bridge_get_system_theme`（后端经 `PluginHandle::get_system_theme` 读取 Windows 注册表 `AppsUseLightTheme`，无需宿主配置解析）
- **运行期变化**：`system-theme-changed` 事件（后端注册表监听驱动，见 `bootstrap::init_system_theme_monitor`）

前端 system 模式仅消费这两个通道；外观配置由后端 `appearance` 组件管理：
- 后端设置变更 → 前端 `applyAppearanceSettings()` 更新 CSS 变量 → 组件自动响应
- `styles/variables.css` 定义所有 CSS 变量的静态默认值
- 暗色模式通过 `html.dark` class 切换，颜色使用 CSS 变量（如 `var(--text-primary)`）
