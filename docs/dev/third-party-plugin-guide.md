# ZeroLaunch 第三方插件开发指南

## 总览

第三方插件以**独立子进程**方式运行，通过 **stdio JSON-RPC 2.0** 与 ZeroLaunch 宿主通信。
支持任意编程语言（Rust、Python、Node.js 等）。

## 插件形态与唤醒契约（重要）

`PluginMetadata.mode` 决定插件形态，**唤醒方式由形态决定，二者不可混用**：

| 形态 | 触发词语义 | 唤醒方式 | 展示 |
|---|---|---|---|
| 行内（`inline`） | **路由触发词**（如 "ev " 命中路由） | 用户输入触发词 + 空格，宿主路由 | 结果列表嵌入搜索窗口（保留搜索栏） |
| 沉浸式（`panel`） | **候选搜索关键字**（如 "ev" 命中候选项） | ① 声明热键（`hotkey`，如 "Ctrl+E"）② 候选项被选中 | 全窗口接管（`CustomPanel` + `keepSearchBar=false`） |

- 行内插件返回 `List`（结果嵌入）或 `CustomPanel`（`keepSearchBar=true` 行内面板）
- 沉浸式插件 **必须** 返回 `CustomPanel`（`keepSearchBar=false`），否则热键唤醒报错
- `trigger_keywords` 语义随形态而变：Inline = 路由触发词；Panel = 候选搜索关键字
  （**不参与路由**，宿主注册时过滤；注入为候选项匹配关键字）
- **候选项自动注入**：启用的沉浸式插件自动成为默认搜索候选项——**不经数据源
  关键字流水线**（`trigger_keywords` 是插件设计好的精确关键字，加上插件名即完成
  匹配，无需优化器派生），且**不参与展示名去重**（插件名与程序名相同不互相丢弃）。
  选中候选项即唤醒面板——无需插件做任何事。实现为宿主内置 `PluginWakeExecutor`：
  统一经 executor 管道（`resolve → execute → wake_plugin`），与普通候选确认路径
  完全一致，无特判分支。图标为插件元数据 data URL（`IconRequest::Data` 直通
  图标链路），`targetType` 为 `"Plugin"`（前端词表键名，无特判消费）

### 面板类型（panel_type）契约

`CustomPanel.panel_type` 由**后端统一规范化**后再下发前端：
- 内置插件：保留自定义 `panel_type`（匹配内置面板组件）
- 第三方插件：**统一为 `third-party:<pluginId>`**（前端按插件 id 注册面板 provider，精确匹配）——插件侧返回任意 `panel_type` 均可，无需感知此前缀

### 沉浸式面板的数据通道

面板内输入查询**不能**走触发词路由（沉浸式插件无触发词），面板内嵌执行与宿主
同 document，统一经 `bridge_query` 显式指定目标插件：
`bridge_query(rawQuery, confirm, panelPluginId)`——有值时后端直调该插件 `query()`
（`search_term` = rawQuery 小写，不剥离前缀），走 `QueryChannel::Panel` 通道
（GUI 进程内只读辅助路径：不参与触发词路由、不改写 GUI 会话、独立版本计数
不与用户输入/CLI 查询竞争——CLI 通道仅保留给 zerolaunch-cli 外部查询）。
响应为 `BridgeQueryResponse`（mode 词表 + 图标已解析为 data URL）。
插件 UI 统一内嵌执行（Shadow DOM + 动态 import `zlplugin://` 资源），
无 iframe、无 postMessage 桥、无 CLI HTTP 依赖。

### 面板主题跟随宿主

自定义面板运行在宿主 Shadow DOM 内，**样式应直接使用宿主 CSS 变量**
（`--bg-primary`、`--text-primary`、`--text-secondary`、`--border-color`、
`--accent-color`、`--primary-color-alpha` 等，定义见 `variables.css` 与
`applyAppearanceSettings`）。自定义属性穿过 Shadow DOM 边界继承，宿主切换
浅色/深色主题时面板自动跟随，无需任何 IPC 或轮询。

插件进程如需主题信息（如按主题调整逻辑），使用宿主主题查询能力：

```rust
let theme = host().get_theme().await?; // "light" 或 "dark"
```

返回宿主**实际生效**主题：配置为 `system` 时由宿主解析系统主题后返回最终值，
插件无需感知（也不需要监听配置——所缺状态按需查询即可）。

## 快速开始（Rust）

### 1. 创建项目

```toml
# Cargo.toml
[package]
name = "my-plugin"
version = "0.1.0"
edition = "2021"

[dependencies]
zerolaunch-plugin-sdk-rust = { path = "../ZeroLaunch-rs/crates/plugin-sdk-rust" }
zerolaunch-plugin-api = { path = "../ZeroLaunch-rs/crates/plugin-api" }
zerolaunch-plugin-protocol = { path = "../ZeroLaunch-rs/crates/plugin-protocol" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
async-trait = "0.1"
```

### 2. 实现 Plugin trait

```rust
use zerolaunch_plugin_sdk_rust::run;
use zerolaunch_plugin_api::*;

struct MyPlugin;

#[async_trait::async_trait]
impl Plugin for MyPlugin {
    fn metadata(&self) -> &PluginMetadata { /* ... */ }
    async fn init(&self, ctx: &PluginContext, handle: Arc<PluginHandle>) -> Result<(), PluginError> { Ok(()) }
    async fn query(&self, ctx: &PluginContext, query: &Query) -> Result<QueryResponse, PluginError> { /* ... */ }
    async fn execute_action(&self, ctx: &PluginContext, action_id: &str, payload: serde_json::Value) -> Result<(), PluginError> { /* ... */ }
}

fn main() {
    run(MyPlugin)
}
```

### 2.1 搜索流水线组件（可选）

第三方插件与内置插件对等，可注册搜索流水线上的全部组件类型：

| 组件 | SDK 注册方法 | 管道位置 | 协议语义 |
|---|---|---|---|
| `DataSource` | `with_data_source` | 候选采集 | `plugin/fetch_candidates` |
| `KeywordOptimizer` | `with_keyword_optimizer` | 关键词扩展 | `plugin/keyword_optimize`（逐候选逐组件，链序由宿主按 `priority` 控制） |
| `KeywordInjector` | `with_keyword_injector` | 别名/上下文关键词 | `plugin/keyword_inject`（逐候选） |
| `SearchEngine` | `with_search_engine` | 搜索打分 | `plugin/calculate_scores`（每查询一次，与内置引擎互斥启用） |
| `ScoreBooster` | `with_score_booster` | 分数增强 | `plugin/booster_boost`（每查询一次）+ `plugin/booster_record`（确认时学习） |
| `ActionExecutor` | `with_executor` | 候选执行 | `plugin/executor_execute` |

```rust
PluginApp::new(MyPlugin)
    .with_score_booster(MySemanticBooster::new())   // ScoreBooster 组件（自己的 component_id）
    .run()
```

关键契约：

- 每个组件都是独立的 `Configurable`（各自的 `component_id` / schema / 设置），
  manifest `provides` 只需声明插件主能力；组件清单由进程 `plugin/get_components` 上报，
  宿主按声明接入对应管道——**无需为组件写 manifest 配置段**。
- `KeywordOptimizer` 的 `uses_context` / `priority` 是**设置可变字段**，宿主经
  `plugin/keyword_optimizer_info` 拉取并缓存，设置变更时刷新——勿在 `ComponentKind`
  中声明静态值。
- 组件故障降级（宿主侧边界）：搜索引擎异常 → 本次查询空结果（禁用即恢复）；
  增强器异常 → 保留引擎原分数；优化器/注入器异常 → 跳过该组件（召回略降）。
- 设置页按 `ComponentType` 通用渲染，第三方组件自动出现在对应分组
  （引擎互斥开关等前端逻辑对第三方引擎同样生效）。

### 3. 编写 manifest.toml

```toml
[plugin]
id = "com.example.my-plugin"
name = "我的插件"
version = "0.1.0"
description = "插件描述"
author = "Your Name <you@example.com>"
minHostVersion = "0.7.0"

[runtime]
# command 相对插件目录（安装后布局：插件目录下的 bin/ 子目录）
command = "bin/my-plugin.exe"

[components]
provides = ["plugin"]

# 可选：插件显示图标（相对插件目录，如 icon.png / icon.svg）。
# 缺失/超限不阻断加载（该插件无自定义图标）；是否展示由宿主按插件形态决定：
# panel 形态插件（mode = "panel"）展示，行内插件（mode = "inline"）不展示。
[icon]
path = "icon.png"
```

> 字段名注意：manifest schema 使用 camelCase（`minHostVersion`、`panelEntry`、
> `startupTimeout` 等）。写成 snake_case 不会报错，但字段会被静默丢弃
> （如 `panel_entry` → 面板不加载），排查时先核对字段名。

### 4. 安装

将编译产物和 manifest.toml 放入：
```
%APPDATA%/ZeroLaunch/plugins/com.example.my-plugin/
├── manifest.toml
└── bin/plugin.exe
```

也可以使用模板自带的打包脚本 `package.py`（Python 3.11+，跨平台，无需额外依赖）一键构建并打成宿主可安装的 zip：

```bash
python package.py          # cargo build --release 后打包到 dist/<plugin-id>-<version>.zip
python package.py --no-build   # 复用已有构建产物
```

随后在设置 → 插件管理 → 安装本地插件中选择该 zip 即可。

## Python 插件开发

Python 插件直接读写 stdin/stdout，遵循 LSP-style Content-Length 帧格式：

```python
import sys, json

def read_message():
    headers = {}
    while True:
        line = sys.stdin.readline().strip()
        if not line: break
        key, val = line.split(":", 1)
        headers[key.strip()] = val.strip()
    length = int(headers["Content-Length"])
    body = sys.stdin.read(length)
    return json.loads(body)

def send_response(id, result):
    payload = json.dumps({"jsonrpc": "2.0", "id": id, "result": result})
    header = f"Content-Length: {len(payload)}\r\n\r\n"
    sys.stdout.write(header + payload)
    sys.stdout.flush()

while True:
    msg = read_message()
    method = msg["method"]
    # Handle initialize, query, execute_action, etc.
```

## 动作执行契约（execute_action 载荷）

插件 `execute_action(action_id, payload)` 收到的 `payload` 形状由**触发通道**决定，
同一动作可能从两种通道进入，插件应同时兼容：

| 通道 | payload 形状 | 场景 |
|---|---|---|
| 候选确认 | `{"candidate_id": <u64>, "query_text": <str>, "user_args": [<str>]}` | 搜索栏结果列表 / 行内参数 / 参数面板确认 |
| 面板动作 | 插件自定义自由 JSON（宿主原样透传） | 面板按键绑定 `Custom` 动作（内嵌执行，宿主 `executeAction` 直接透传） |

候选确认通道的 `candidate_id` 即结果项的 `ListItem.id`；面板动作通道可携带
面板内状态（如选中项的完整路径），不依赖候选缓存。

## 调试

- 查看日志：`%APPDATA%/ZeroLaunch/plugin-logs/<plugin-id>.log`
- 使用 Plugin Inspector（设置 → 插件检查器）
- stderr 输出会被自动收集
