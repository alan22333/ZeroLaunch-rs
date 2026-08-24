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
### 第二次迭代（2026-08-23，已完成）

**Everything 沉浸式面板改造**（顺带验证沉浸式面板全链路）：

1. **插件侧**（`zerolaunch-plugin-everything/`）：
   - 形态改为 `PluginMode::Panel` + 热键 `Ctrl+E`（panel 形态才注册热键表，前端 useKeyboardRouter 唤醒）。
   - `query()` 分流：`search_term` 为空（热键唤醒）→ `CustomPanel{panel_type:"everything", keep_search_bar:false}`（沉浸式接管整个窗口）；非空（面板内经 CLI 通道的 `ev <内容>` 查询）→ 返回 `List` 搜索列表。两种形态共用同一入口。
   - `execute_action` 载荷兼容双通道：面板动作（pluginAction 自由 JSON，`{"path": ...}`）与候选确认（`candidate_id` → result_cache 回退）。
2. **沉浸式面板 UI**（`ui/panel.mjs`，iframe 内自包含）：
   - 搜索框自动聚焦 + 200ms 防抖 → `host.callHost('query', {rawQuery: 'ev <内容>'})`（CLI HTTP 通道，触发词路由）→ 渲染列表（title/subtitle）。
   - 按键在 iframe 内处理（iframe 键盘事件不冒泡到宿主窗口）：↑↓ 导航、Enter 打开、Ctrl+U 路径匹配切换、Esc 退出、双击打开。
   - 动作执行 → `sendToHost({type:'action-trigger', action, args})`；退出 → `sendToHost({type:'exit'})`。
3. **宿主桥接（本迭代新增的宿主能力）**：`ThirdPartyPanelHost.vue` 处理 iframe 消息——`action-trigger` → `bridge_confirm` 的 `pluginAction` 通道（args 自由 JSON 原样透传，归属与代际由后端校验）；`exit` → 隐藏窗口（与宿主 Esc 语义一致）。此前第三方 iframe 面板只有数据下发，无动作执行通道（CLI HTTP 仅只读）。
4. **验证**：`cargo check` / `vue-tsc --noEmit` 零错误；zip 含 `ui/panel.mjs`；`manifest.toml` 新增 `[ui] panelEntry`。

**已知限制（记录）**：宿主 `PanelInteraction.bindings` 对第三方 iframe 面板无效（iframe 内按键宿主收不到），面板按键必须自包含；面板内文本硬编码中文（插件 i18n 语言包机制不覆盖 iframe 面板，后续迭代可经 `CustomPanel.data` 下发翻译文本）。

### 第三次迭代（2026-08-23，已完成）

**插件 UI 内嵌执行方案（替代 iframe）+ 生产可用性修复**：

1. **决策：全信任模型 + 宿主内嵌执行**。插件进程本身全权限（无权限模型），iframe 的 JS 隔离名存实亡；改为 Shadow DOM 容器 + 动态 `import('zlplugin://<id>/ui/panel.mjs')` 内嵌执行（`ThirdPartyPanelHost.vue` 重写）：
   - 键盘事件自然冒泡到宿主窗口 → `PanelInteraction.bindings`「声明即接管」对第三方插件生效（everything 声明 Esc→GoBack、Ctrl+U→Custom toggle_path_match，见 `interaction_policy`）
   - i18n 同步直查（`host.t(key)` 自动补插件 id 前缀，走 vue-i18n 插件语言包，支持插值）
   - 动作/数据直连 IPC（`executeAction` → pluginAction 通道；`query` → CLI 只读通道）；旧 iframe 消息协议（action-trigger/exit）经 `sendToHost` 兼容映射
   - Shadow DOM 保留样式隔离；`__zlplugin_iframe__.html` 仍被设置面板 iframe 使用，保留
2. **生产可用性修复（两个隐藏 bug，此前 iframe 面板从未真正可运行）**：
   - `zlplugin://` 协议响应缺 CORS 头：动态 import 跨源模块被 Chromium 拦截 → `lib.rs` 协议注册补 `Access-Control-Allow-Origin: *`（执行面由 CSP script-src 白名单约束）
   - `src-ui/public/__zlplugin_iframe__.html` 不在 vite publicDir（dev/prod 均 404）→ `git mv` 到根 `public/`
3. **everything 面板迁移**：`panel.mjs` 文本全部 i18n 化（`host.t`，11 个新键）；Esc/Ctrl+U 改由宿主 bindings 处理；↑↓/Enter/双击面板内。
4. **验证**：vue-tsc 零错误；vite build 成功且 dist 含 iframe html；cargo check 零错误；插件 zip 含新 panel.mjs + 扩展 i18n。

**已知限制（记录）**：面板数据通道已于第五轮迁移为宿主 IPC（`bridge_panel_query`），CLI 退回纯只读宿主查询；面板查询响应为裸 QueryResponse 形状（已文档化）。

### 第四次迭代（2026-08-23，已完成 — 形态契约落地）

**插件形态与唤醒契约（用户权威定义，已写入代码 + 文档）**：

| 形态 | 触发词 | 唤醒方式 | 展示 |
|---|---|---|---|
| 行内（inline） | 有 | 触发词 + 空格路由 | 列表嵌入搜索窗口 |
| 沉浸式（panel） | **无**（宿主注册时过滤） | ① 热键 ② 候选项选中 | 全窗口接管 |

落地改动：
1. **触发词过滤**：`session_dispatcher::register_plugin_with_triggers` 对 Panel 形态插件的 trigger_keywords 不写入路由索引（契约权威在后端）；`PluginMetadata.trigger_keywords` / `PluginMode` 文档注释同步。
2. **panel_type 契约**：新增 `normalize_panel_type`——第三方插件 CustomPanel.panel_type 后端统一重写为 `third-party:<pluginId>`（前端 provider 精确匹配；此前插件自定义 panel_type 与注册的 matchType 对不上 → 面板 fallback"插件面板不可用"）。应用于 bridge_query 与 wake_plugin 两处消费点。
3. **面板数据通道**：新增 CLI `POST /v1/panel/query {pluginId, rawQuery}`——直调插件 query()，不经触发词路由、不改写 GUI 会话（只读约束）；`ThirdPartyPanelHost.query()` 与 everything panel.mjs 改走此端点（不再拼触发词前缀）。
4. **everything 插件**：trigger_keywords 置空（契约），query() 分流保留（空 raw_query → 面板；非空 → 搜索）。
5. 文档：`third-party-plugin-guide.md` 新增"插件形态与唤醒契约"章节；plugin-api types.rs 注释。

**候选项唤醒机制（同轮补充完成）**：
1. `ExecutionTarget::Plugin(String)` + `TargetType::Plugin`（plugin-api 公开契约，serde 键名 `"plugin"`/`"Plugin"`，带注释）。
2. `refresh_candidates` 注入：启用的 Panel 形态插件自动成为候选项（名称 = 插件名，target = 插件 id，图标占位）；id 由 CachedCandidateData 统一分配、target 去重。
3. `execute_candidate` 拦截：`ExecutionTarget::Plugin` 选中 → `wake_plugin`（不经 executor 管道，契约路由优先于 ExecutorRegistry 唯一入口）。
4. `bridge.rs` List 包装：`target_type == "Plugin"` 时图标直接透传插件元数据 data URL（subtitle 即插件 id），不走图标提取。

### 第五次迭代（2026-08-23，已完成 — iframe 全面移除 + 面板通道 IPC 化）

1. **iframe 产物全部移除**：`public/__zlplugin_iframe__.html`、`postMessageBridge.ts` 删除；`ThirdPartyPanelHost` 的旧消息协议兼容层（`sendToHost`）删除；**设置面板也迁移内嵌**（`ThirdPartySettingsHost` 动态 import + Shadow DOM，host API：`onSettingsUpdate` / `save` / `t`）。插件 UI 全链路无 iframe、无 postMessage 桥。
2. **面板数据通道 IPC 化**：删除 CLI `/v1/panel/query` 端点（iframe 时代遗留）；随后按用户判断**合并进 `bridge_query`**——新增可选参数 `panel_plugin_id`（显式指定目标插件直调，`Cli` 只读门控：不经触发词路由、不改写会话、不与用户输入竞争版本号），响应统一为 `BridgeQueryResponse`（**图标解析免费**，面板列表获得图标）。独立命令 `bridge_panel_query` 删除。CLI HTTP 退回纯只读宿主查询（彻底落实"CLI 只读"约束）。
3. 验证：vue-tsc / cargo check 零错误；生产构建通过且 dist 无 iframe html 产物。

### 第六次迭代（2026-08-24，已完成 — 候选项唤醒重构：消除 5 处特判）

**用户反馈**：候选项唤醒设计不优雅（插件目录移出仓库后审视宿主代码）。
**问题清单**：插件候选伪造 SearchCandidate（id=0、空图标、trigger_keywords 空置）；走数据源 collect 管道（关键字优化器派生对用户设计好的触发词无意义，且 **display-name 去重会误杀同名插件候选**）；execute_candidate if-let 旁路 executor 管道；ListItem 动作注入特判；bridge.rs 图标特判 + subtitle 承载 plugin_id（UI 副标题显示插件 id）。

**重构方案**：
1. `IconRequest::Data(String)` 新变体（plugin-api）：data URL/base64 解码直通图标链路（`IconExtractor::extract` 新增分支，缓存键经 bincode 哈希天然兼容）——候选图标原生携带，bridge 图标特判删除。
2. **插件候选不经 collect**：`build_plugin_candidates` 直接构造完整候选（icon=Data(meta.icon)、keywords=触发词+名称）；`CachedCandidateData::add_plugin_candidate` 仅按 target 去重（跳过展示名去重——插件名与程序同名不互相丢弃）；bootstrap 启动采集同步同构。
3. **`PluginWakeExecutor`**（plugin_framework 宿主内置执行器，持 `Weak<SessionDispatcher>`，bootstrap 手动注册）：supported_target_types=[Plugin]、actions=[open: common.open]、execute → wake_plugin——确认路径统一 resolve → execute，execute_candidate 旁路删除、ListItem 动作注入特判删除（get_actions 自然返回）。
4. subtitle 不再承载 plugin_id（插件候选副标题置空，UI 不再显示插件 id）。
### 第七次迭代（2026-08-24，已完成 — 通道语义纠正 + 副标题信息化）

**用户反馈**：
1. `QueryChannel::Cli` 门控被面板查询借用是架构错误——Cli 通道语义是"zerolaunch-cli 外部查询"，面板查询是 GUI 进程内只读查询，两者共享版本域互相污染。
2. 插件候选副标题置空不友好——"有一些提供总比空的好"。

**修复**：
1. **新增 `QueryChannel::Panel` 变体**（plugin-api，serde 键名 `panel`，变体注释完整）：GUI 进程内只读辅助通道——不改写会话（route_query 的 `== Ui` 判断天然覆盖）、独立版本计数（`panel_query_revision` 新字段，三通道互不竞争）。`bridge.rs` 面板分支由 `Cli` 改为 `Panel`；CLI HTTP 保持 `Cli` 通道。插件侧 `== QueryChannel::Ui` 的判断（translator/calculator 写共享状态）对面板查询自动为 false——面板查询保持只读，语义正确。
2. **副标题信息化**：插件候选 subtitle = 插件描述（可读性优先），描述缺失时兜底 `plugin id: {id}`——不再空副标题。
3. 验证：cargo check（src-tauri + workspace）/ vue-tsc 零错误；91 测试通过。

### 第八次迭代（待做）

1. 语义搜索插件（方案 A：触发词模式，用 `host().enumerate_apps()` 建索引 + 本地 embedding）。
2. 安装到宿主做 CDP 真实验证（设置 → 插件管理 → 安装本地插件 → `ev <查询>`）。
3. 模板待补：`.gitignore`（target/dist）、CI（.github，当前模板没有）、`minHostVersion` 按新 API 需求调整。
4. 提交当前大量未提交工作区变更（建议按轮次/领域拆分 commit）。
5. 发布前置：license `GPLv3` 修正为 `GPL-3.0-only`（crates.io 阻塞项）。

### 序列化键名契约固化（code review 第七次迭代补充）

**决策**：SDK 未发布 → 破坏性变更窗口内**固化全部跨 RPC 枚举键名**（serde-rename 契约），发布后永不改名。

- `QueryChannel`：补 `#[serde(rename = "ui"/"cli"/"panel")]`（此前默认键名 Ui/Cli 未显式标注；键名变化在破坏性窗口内完成，旧 SDK 二进制不存在）
- `TargetType`：补全 7 个变体显式标注（Path/App/File/Url/Command/BuiltinCommand/Plugin，与 as_str() 前端词表一致；此前全部依赖默认键名）
- 新增契约测试 `query_channel_and_target_type_serialize_with_stable_keys`（plugin-api）：键名以测试固化，防未来误改
- 错误类型（ExecutionError/RegistrationError/PluginError）为 thiserror（不序列化），serde-rename 规则不适用，不标注

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
