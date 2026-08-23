# 插件生态拆分与 SDK 发布 — 里程碑记录

> 状态：**进行中**（2026-08-23 首次记录，预计多轮迭代完成）
> 本文是交接文档：记录目标、当前事实、分析结论与待决事项。新接手者先读本文，
> 再读 `docs/dev/third-party-plugin-guide.md` 与 `docs/design/plugin-sdk.md`。

## 1. 目标（一次多轮迭代的整体规划）

1. **验证 SDK 稳定性**：基于当前代码复现两个历史插件——Everything 插件、语义搜索插件。开发中发现的接口缺陷就地修复，并同步修复 `plugin-template/`。
2. **拆分 `plugin-template/`** 为独立 GitHub 仓库（启用 template 标志）。
3. **发布 SDK crates 到 crates.io**：`zerolaunch-plugin-api`、`zerolaunch-plugin-protocol`、`zerolaunch-plugin-host`、`zerolaunch-plugin-sdk-rust`。
4. **从 template 创建两个插件仓库**（`zerolaunch-plugin-everything`、语义搜索插件），移植已开发代码上传。

## 2. 当前仓库状态（已验证事实）

- Cargo workspace 版本 **1.2.0**；4 个 SDK crate 均 `version.workspace = true`，无 `publish = false`。
- `plugin-template/` 被 workspace `exclude`（根 `Cargo.toml`），是独立 crate，依赖为 path 形式（`../crates/...`）。文件：`Cargo.toml`、`manifest.toml`、`package.py`、`src/main.rs`、`ui/panel.mjs`、`i18n/{zh-Hans,en}.json`、`Cargo.lock`。
- 模板插件骨架：`HelloWorldPlugin`（ComponentType::Plugin，mode: Inline，trigger keywords "hello"/"hw"），i18n 键为裸键（宿主按 plugin id 加命名空间，改名插件无需改 i18n JSON）。
- **4 个 crate 名在 crates.io 均未被占用**（2026-08-23 实测 404），无需抢注。
- 当前代码中已无 everything / 语义搜索残留（grep 确认）。

### 历史插件代码位置（git history，供复现参考）

| 插件 | 旧文件 | 删除 commit | 技术栈 |
|---|---|---|---|
| Everything | `src-tauri/src/modules/everything/mod.rs` + `config.rs` | 5043dfb | `everything-rs 0.1.10` + `everything-sys-bindgen 0.1.5`（均 crates.io，**x86_64 only**）+ `Everything64.dll`（曾随 src-tauri 分发，现已不在仓库） |
| Everything 前端 | `src-ui/input_states/everything_shortcut_handler.ts` | f966b50 | 独立页面：专用搜索框、上下导航、Enter 打开、Ctrl+U 路径匹配切换 |
| 语义搜索 | `src-tauri/src/modules/program_manager/semantic_backend.rs` + `semantic_manager.rs` | 2dce34f | `EmbeddingGemmaModel`（feature "ai"）、模型由主程序 ModelManager 管理（该代码已整体删除） |

- Everything 旧交互：唤醒 Everything IPC 搜索 → 返回文件路径列表（blake3 hash 作 id）→ `cmd /C start "" <path>` 打开；配置项：`sort_threshold`、`sort_method`、`result_limit`、`enable_path_match`（Ctrl+U）。
- 语义搜索旧本质：**程序搜索的全局重排器**——`search_engine.rs:95-104` 对每个用户输入算 embedding，与每个程序预计算 embedding 求相似度参与排序（`compute_similarity`，ndarray + Gemma 模型）。

## 3. 关键分析结论

### 3.1 Everything 插件 — 当前 SDK 可完整复现（无阻塞）

| 旧行为 | 当前 SDK 映射 |
|---|---|
| Ctrl+E 唤起独立页面 | `PluginMetadata.hotkey` + `mode: Panel`（`crates/plugin-api/src/plugin/types.rs`） |
| 输入即时出结果 | 触发词路由（`src-tauri/src/plugin_framework/session_dispatcher.rs` 触发词调度分支）→ `QueryResponse::List` |
| 文件图标 | `ListItem.icon` → `host().get_icon()` RPC（`crates/plugin-sdk-rust/src/host_proxy.rs`） |
| Enter 打开文件 | `execute_action` → `host().shell_open()` |
| Ctrl+U 路径匹配切换 | `ResultAction.shortcut_key = "Ctrl+U"` → `execute_action` 翻转开关，后续查询走 `Everything_SetMatchPath` |
| 4 个配置项 | `Configurable::setting_schema`（模板 `main.rs` 有注释示例） |

注意点：

1. **`Everything64.dll` 必须随插件分发**（`everything-sys-bindgen` 运行时需要，与 exe 同目录）。当前 `package.py` 只打包 manifest/bin/ui/i18n/icon，**无额外文件支持 → 模板缺陷 #1，开发期需修复并同步回模板**。
2. x86_64-only：`cfg(target_arch)` 门控（旧代码有先例）。

### 3.2 语义搜索插件 — SDK 架构性缺口（计划最大风险点）

- 第三方插件**只能注册三类组件**：`Plugin`（触发词路由）、`DataSource`（候选池）、`ActionExecutor`（`session_dispatcher.rs` 的 `PluginRegistered` 事件分支；SDK runtime 仅提供 `with_data_source`/`with_executor`）。
- `SearchEngine` / `ScoreBooster` **只有内置注册表能提供**（`builtin_registry.rs`）→ **第三方插件无法挂进默认搜索管道做全局重排**，与旧版行为不等价。

两条出路：

- **方案 A（先做）**：降级为触发词模式——关键字（如 `sem `）→ `host().enumerate_apps()` 建应用索引 → 本地 embedding 相似度排序 → `List` 结果 → `shell_open`。当前 SDK 可完整实现，用于验证 SDK 稳定性。
- **方案 B（后续独立立项）**：把"第三方 SearchEngine/ScoreBooster 贡献点"作为 SDK 缺陷补齐——plugin-api 序列化适配 + protocol 新 RPC 方法 + host 端 adapter 注册进 search pipeline + manifest 扩展，跨 4 crate + 主程序。

未决问题：模型权重分发——zip 内置（几百 MB 不现实）vs 首启下载（需插件本地可写目录；当前 SDK 仅有 host `resource_put/get`，几百 MB 是否合适待评估）。

### 3.3 发布阻塞项（crates.io 硬性要求）

1. **`license = "GPLv3"` 不是合法 SPDX 表达式，crates.io 直接拒绝**。须改为 `"GPL-3.0-only"`（workspace `Cargo.toml` 的 `[workspace.package]`）。
   - ⚠️ 附带决策：SDK 用 GPL-3.0 意味着链接它的第三方插件须 GPL 兼容——对插件生态是法律层面的阻碍，**待用户决策**。
2. 4 个 crate 缺 `repository` 字段（发布仅 warning，建议补上）。
3. 发布前对每个 crate 跑 `cargo publish --dry-run` 全链路验证。
4. `plugin-host` 带 `[[bin]] fixture_plugin` 测试 fixture，会随 crate 发布，无害但留意。

### 3.4 顺序修正（相对初版计划）

**发布 crate 必须先于 template 分离**：template 独立成仓库后，`Cargo.toml` 必须从 path 依赖改为 crates.io 版本依赖，crate 未发布则 template 仓库无法构建（CI 必红）。

最终顺序：

1. 仓库内完成两插件开发 + API 修复（保持 path 依赖）
2. `cargo publish` 4 个 crate（先 dry-run）
3. 分离 template（改版本依赖）+ 建 GitHub template 仓库（勾选 template 标志）
4. 从 template 建两个插件仓库（手动改名：repo 名 / crate 名 / manifest id / main.rs 内 id）
5. 移植已开发代码进各自仓库（path → 版本依赖），上传

版本策略：开发期不发布，完成后一次性发布，发布版本即"验证过的版本"；插件/template 依赖写 caret 下限（如 `"1.2"`）而非精确 pin。

## 4. 待决事项（下次迭代开始时确认）

- [ ] 语义搜索走方案 A 还是直接立项方案 B（建议先 A 验证 SDK）
- [ ] SDK 许可证策略（GPL-3.0-only vs 其他，影响第三方生态）
- [ ] 模型权重分发方式
- [ ] "发布 template"的形态：GitHub template 仓库（推荐）vs crates.io template crate（不推荐，无 cargo-generate 支持）

## 5. 迭代记录

### 第一次迭代（2026-08-23，已完成）

1. **复制改名**：`zerolaunch-plugin-everything/`、`zerolaunch-plugin-ai-search/`（均为仓库内独立 crate，path 依赖 `../crates/`；已加入根 `Cargo.toml` 的 workspace `exclude`）。ai-search 为改名骨架（id `com.ghost-him.ai-search`，触发词 `sem`），语义实现留待后续迭代。
2. **Everything 插件完成**（`zerolaunch-plugin-everything/`，id `com.ghost-him.everything`，触发词 `ev` / `every`，Inline 模式）：
   - 4 配置项 schema（sort_threshold / sort_method 26 枚举 / result_limit / enable_path_match），i18n 三套键值（en + zh-Hans）。
   - 查询链路：`query.search_term` → `spawn_blocking` + `Mutex` 串行化（Everything SDK 全局状态）→ blake3 哈希作候选 id → `ListItem`（Path 图标，宿主按需取）→ 动作：打开（Enter 默认）、路径匹配切换（Ctrl+U 提示）、打开所在文件夹。
   - 动作执行：候选确认载荷 `{candidate_id, query_text, user_args}`（`session_dispatcher.rs` 契约）经 result_cache 还原路径 → `host().shell_open` / `shell_open_folder`。
   - **打包缺陷 #1 已修复**：`package.py` 新增 `extra/` 约定目录——内容原样并入 zip 根（DLL 与 `bin/` 同级，子目录保持相对结构）；宿主以 plugin_dir 为子进程 cwd（`plugin-host/src/transport/stdio.rs`），DLL 放插件目录根即可被加载（已验证 exe PE 导入 `Everything64.dll`）。
   - 验证：`cargo check` 零错误；`dist/com.ghost-him.everything-0.1.0.zip` 含 manifest.toml + bin/exe + i18n×2 + Everything64.dll（94336 B，取自 everything-sys-bindgen 0.1.5 内置副本）。
3. **已知限制**：前端 `ResultAction.shortcutKey` 仅为展示提示，不自动分发按键——Ctrl+U 实际通过 Tab 选中 / 点击 / Ctrl+数字 触发；快捷键自动分发属后续迭代可选增强。

### 第二次迭代（待做）

1. 语义搜索插件（方案 A：触发词模式，用 `host().enumerate_apps()` 建索引 + 本地 embedding）。
2. 安装到宿主做 CDP 真实验证（设置 → 插件管理 → 安装本地插件 → `ev <查询>`）。
3. 模板待补：`.gitignore`（target/dist）、CI（.github，当前模板没有）、`minHostVersion` 按新 API 需求调整。

## 6. 相关文件索引（改动前必读）

| 目的 | 路径 |
|---|---|
| 第三方插件数据契约 | `crates/plugin-api/src/plugin/types.rs`（QueryResponse/ListItem/PluginMetadata/PluginMode） |
| SDK 运行时与宿主 RPC | `crates/plugin-sdk-rust/src/runtime.rs`、`host_proxy.rs` |
| 触发词路由 / 第三方组件注册 | `src-tauri/src/plugin_framework/session_dispatcher.rs` |
| 插件打包规范 | `.omp/rules/zerolaunch-plugin-packaging.md`、`plugin-template/package.py` |
| 第三方插件开发指南 | `docs/dev/third-party-plugin-guide.md` |
| 插件架构设计 | `docs/design/third-party-plugin-architecture.md`、`docs/design/plugin-sdk.md` |

## 7. 给新人的快速认识

- 架构：Cargo workspace（6 crate + src-tauri + src-ui），依赖方向 `plugin-api ← plugin-protocol ← plugin-host ← src-tauri`，禁止反向依赖；详见根目录 `AGENTS.md`。
- 第三方插件 = **独立子进程 + stdio JSON-RPC（LSP Content-Length 帧）**，与内置插件（进程内 inventory 注册）正交。
- 前端是薄展示层，业务逻辑、平台操作 MUST 走 IPC 委托后端（见 `.omp/RULES.md` 前后端职责边界）。
- 本仓库另有 TTSR 条件规则（`.omp/rules/`），改到相关文件时会自动注入。
