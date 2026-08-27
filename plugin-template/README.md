# ZeroLaunch 第三方插件模板

本模板是 ZeroLaunch 第三方 Rust 插件的最小骨架，覆盖开发、调试、打包、安装全流程。

## 项目结构

```
plugin-template/
├── Cargo.toml          # 依赖 zerolaunch-plugin-sdk-rust / plugin-api / plugin-protocol
├── manifest.toml       # 插件清单（id、形态、热键、面板入口），打包时位于 zip 根
├── package.py          # 一键打包脚本（cargo build --release + 生成安装 zip）
├── src/main.rs         # 插件实现（Plugin + Configurable trait）
├── ui/                 # 自定义面板（沉浸式/行内插件可选）
└── i18n/               # 语言包（host 加载时合并进翻译目录，t_key() 自动带插件 id 前缀）
```

依赖方向：插件只依赖 `zerolaunch-plugin-api`（trait/类型）与 `zerolaunch-plugin-sdk-rust`（`run()`、`host()`），不依赖 Tauri/宿主源码。

## 开发流程

1. **改标识**：`manifest.toml` 的 `id`（如 `com.example.hello-world`）与 `src/main.rs` 中 `ComponentCore::new` / `PluginMetadata.id` 保持一致；两者及 `Cargo.toml` 的 `version` 三处版本号需同步。
2. **实现 `Plugin` trait**：
   - `metadata()`：插件元数据——注意 `mode` 决定形态：`Panel`（沉浸式，热键/触发词唤醒后接管窗口）或 `Inline`（行内，触发词前缀路由）。
   - `query()`：接收用户输入返回结果。两种响应形状：
     - `QueryResponse::List`：标准搜索结果（搜索栏/CLI 输出）。
     - `QueryResponse::CustomPanel`：自定义面板响应，`data` 可承载**任意 JSON**，面板 UI 完全自定义（参考 Everything 插件按需渲染）；`keep_search_bar` 决定是否保留搜索栏。
   - `execute_action()`：动作执行（打开文件等经 `host()` 平台 API）。
3. **`Configurable` trait**：`setting_schema()` 声明设置项（宿主设置页自动渲染），`apply_settings()` 应用返回值，`get_settings()` 提供当前值。
4. **i18n**：所有面向用户的文本用 `t_key("key")` 生成命名空间键，`i18n/zh-Hans.json`、`en.json` 提供译文；面板侧用 `host.t(key)`（同键）。
5. **自定义面板**（可选）：`manifest.toml` 的 `[ui] panelEntry` 指向 `ui/panel.mjs`，导出 `mount(rootEl, host)`；锚定宿主 Shadow DOM 内执行，样式直接用宿主 CSS 变量（`--bg-primary`、`--text-primary` 等）即可自动跟随宿主主题。

## 调试与验证

- **插件日志**：`%USERPROFILE%/.ZeroLaunch-rs/plugin-logs/<plugin-id>.log`（与宿主日志分离，可直接检查查询/错误）。
- **CLI 查询**：宿主运行时可 `zerolaunch-cli.exe query "ev xxx"` 直查插件响应形状（加 `--json` 输出原始 JSON，适合验证 `CustomPanel` 载荷）。
- **修改后冒烟**：`cargo check` 零错误；面板改动可在宿主预览（设置页重新加载插件）。

## 发布

```bash
python package.py          # 等价于 cargo build --release 后打包（无 Python 可用 uv run --python 3.12 python package.py）
python package.py --no-build   # 复用现有产物直接打包
```

生成 `<plugin-id>-<version>.zip`：`manifest.toml` 必须位于 zip 根，`extra/` 目录内容并入 zip 根（与 exe 同目录的运行时文件放这里）。

安装：设置 → 插件管理 → 安装本地插件，选择 zip；或手动解压到 `%USERPROFILE%/.ZeroLaunch-rs/plugins/<plugin-id>/` 后重新加载。
