# 更改档案：embedding 契约重构——task_type 必填化与模板档案内置化

- **基线**：`main@61d471c`（行号均以此为准）
- **状态**：已评审定案，待实施
- **范围**：`crates/plugin-api`（模型契约）、`src-tauri/src/core/model/`（宿主模型核心）、`src-tauri/src/builtin_plugin/config/`（模型配置组件）、`src-ui/i18n/`（三语言文案）
- **兼容性策略**：**不做旧代码兼容**。旧配置字段静默丢弃，旧 embedding 缓存随 namespace 变化自然失效（见 §7）

---

## 1. 背景与目标

当前 `task_type` 是 `ModelEmbeddingRequest` 的可选参数（`None` = 裸传），模板解析为三层优先级：**用户配置 → 内置档案 → gemma 兜底**（`model_profiles.rs:12-15`）。该设计存在以下问题：

1. 模板是模型实现细节（qwen3 的 Instruct 格式、gemma 的 task 前缀），把它的编辑权交给终端用户属于知识归属错位——内置模板尚且会写错（61d471c 修复的 gemma 文档侧占位符误用），用户手写错误率更高；
2. `None`（裸传）与 `PlainText`（裸传任务）语义重复，是两条路径做同一件事，且产生等价向量却两个缓存条目；
3. 无档案模型的 gemma 兜底对 bge/gte/m3e 等未见过该前缀的模型不是中性操作，效果可能劣化；
4. 调用方不声明任务时，非对称检索的 query/document 两侧无法保证模板配套，检索质量静默劣化。

**目标**：模板归档案库（唯一来源）、任务归调用方（必填声明）、用户只管选模型。

## 2. 已确认的决策

| # | 决策 | 内容 |
|---|------|------|
| D1 | 无档案模型一律裸传 | 删除 gemma 兜底路径。模型无内置档案时，**所有**任务（含 `retrieval_query` 等）原样透传 input，不套任何前缀。`GEMMA_FALLBACK` 常量仅作为 gemma-embedding 档案自身的模板表保留 |
| D2 | task_type 必填 | 序列化形状保持 `Option<String>` + `#[serde(default)]` 不变，宿主入口将 `None`/空串/未知值统一拒绝为 `ModelError::InvalidRequest`，保证插件拿到明确业务错误而非反序列化泛型错误 |
| D3 | 删除 `EmbeddingCapability::TaskType` 能力位 | 模板组装是宿主侧行为（provider 只收到拼好的最终字符串），模型层面不存在"不支持 taskType"；强制必填后能力门控必然失效。保留 `OutputDimensions` |
| D4 | 删除 `template_args` 契约字段 | 61d471c 后所有内置模板只引用 `{0}`，无任何调用方传参。gemma 文档侧未来若需 title，由宿主从候选元数据自取，不跨 RPC 传 |

另含一项连带修复（§6）：`cosine_similarity` 实现真余弦 + 修正 `types.rs` 的"归一化向量"假注释。

## 3. 重构后的行为规范（权威定义）

### 3.1 task_type 必填规则

宿主 embedding 入口（`ModelManager::embedding`）按以下顺序校验：

1. `task_type` 为 `None` 或空串 → `InvalidRequest("embedding 调用必须指明 task_type（retrieval_document / retrieval_query / semantic_similarity / classification / clustering / plain_text）")`
2. 非上述六个序列化名之一 → `InvalidRequest("未知的 task_type: {task}")`（沿用现有文案）
3. 校验通过后由 `compose` 层解析为 `SemanticTask` 枚举继续处理

### 3.2 模板解析规则（唯一规则，无兜底）

```
profile_for(model_id) 命中档案：
    task 在档案 task_templates 中命中 → 渲染模板（仅 {0} 占位符替换为 input 文本）
    task 在档案中未命中（如未来新增枚举但档案未更新）→ 裸传
profile_for 未命中（无档案模型）：
    所有任务一律裸传
```

- **不存在**用户模板层、**不存在** gemma 兜底层、**不存在** "不传 task_type 裸传" 分支
- 模板渲染只支持 `{0}`；模板中的其他字面量（含 `{1}` 等）原样输出
- `SemanticTask` 枚举与六个序列化名保持不变（`model_profiles.rs:26-48`）

### 3.3 行为对照表

| 场景 | 旧行为 | 新行为 |
|------|--------|--------|
| 不传 `task_type` | 裸传 | **InvalidRequest 拒绝** |
| `task_type = "plain_text"` | 档案/兜底模板 `{0}`（等效裸传） | 不变（所有模型裸传） |
| 无档案模型 + `retrieval_query` | gemma 兜底模板 `task: search result \| query: {0}` | **裸传** |
| 有档案模型 + 任意任务 | 用户配置 > 档案 > 兜底 | **档案（唯一来源）** |
| 模型未勾 taskType 能力 + 带 task_type | InvalidRequest（能力门控） | 能力位已删除，行为只由档案决定 |
| 请求携带 `template_args` | 参与模板渲染 | 字段已删除，多余 JSON 键被 serde 忽略 |

## 4. 删除清单（必须删除，不得保留）

### 4.1 兜底路径（决策 D1 的直接要求）

| 位置 | 删除内容 |
|------|----------|
| `model_profiles.rs:183-189` | **`gemma_fallback_template()` 函数整体删除**——无模板时不兜底，此函数是兜底的唯一入口 |
| `compose.rs:133-135` | `template_for(...).or_else(\|\| gemma_fallback_template(task)...)` 的 `.or_else` 兜底调用，以及 `expect("模板查询恒命中（回退保底）")`——新逻辑下 `template_for` 返回 `None` 即裸传，**永不 panic** |
| `model_profiles.rs:88-93` | `GEMMA_FALLBACK` 的文档注释中"无档案模型的 gemma 回退模板"语义——常量本体保留（它是 gemma 档案模板表），**建议改名 `GEMMA_TEMPLATES`** 并重写注释为"gemma-embedding 档案模板表" |
| `model_profiles.rs:12-15` | 模块头注释中"用户配置 → 内置档案 → gemma 兜底"三层描述、"无档案模型且 task_type 非空时使用 gemma 中性前缀模板，仅 task_type 为空时原样透传"整段 |

### 4.2 用户模板配置层（连同其全部消费链）

| 位置 | 删除内容 |
|------|----------|
| `settings.rs:101-110` | `TaskTemplateItem` 结构体整体删除 |
| `settings.rs:94-98` | `EmbeddingModelConfig.task_templates` 字段删除 |
| `settings.rs:119` | `Default` impl 中的 `task_templates: Vec::new()` 行 |
| `model_profiles.rs:167-181` | `template_for()` 的 `user_templates` 参数及用户配置优先分支（:172-174） |
| `model_profiles.rs:191-204` | `auto_task_templates()` 整体删除（用途是填充用户配置） |
| `model_profiles.rs:206-234` | `apply_profile_defaults()` 整体删除 |
| `model_profiles.rs:236-250` | `apply_profiles_to_entries()` 整体删除 |
| `model_profiles.rs:17` | `use super::settings::ModelEntryConfig`（删除后无引用） |
| `model_openai_config.rs:336-337` | `apply_settings` 中 `apply_profiles_to_entries` 调用及 :336 注释 |
| `model_ollama_config.rs:118-119` | 同上 |

### 4.3 task_type 可选分支

| 位置 | 删除内容 |
|------|----------|
| `compose.rs:128-129` | `match task_type { None => Ok(input.to_vec()) }` 分支——必填后不可达 |
| `compose.rs:69-86` | `validate_embedding_request()` 整体删除（TaskType 能力门控 + Option 校验的旧形态） |
| `mod.rs:190` | 对旧 `validate_embedding_request` 的调用（由新校验函数替代，见 §5） |

### 4.4 `EmbeddingCapability::TaskType` 能力位（决策 D3）

| 位置 | 删除内容 |
|------|----------|
| `types.rs:59-63` | `EmbeddingCapability::TaskType` 变体及其文档注释；枚举仅剩 `OutputDimensions` |
| `model_profiles.rs:79-86` | `EmbeddingModelProfile.capabilities` 字段（唯一用途是自动勾选 TaskType），及 :19 的 `EmbeddingCapability` import |
| `model_profiles.rs:123,150-151` | PROFILES 中 qwen3/gemma 档案的 `capabilities: &[EmbeddingCapability::TaskType]` |
| `model_openai_config.rs:213-214` | `embedding_capabilities` multi_select 的 `("taskType", ...)` 选项 |
| i18n 三语言 | `model-entry.capabilities.task_type` 键（zh-Hans.json:720 等） |

### 4.5 `template_args` 契约字段（决策 D4）

| 位置 | 删除内容 |
|------|----------|
| `types.rs:216-219` | `ModelEmbeddingRequest.template_args` 字段 |
| `mod.rs:191-197` | template_args 与 input 数量一致性校验 |
| `mod.rs:212` | `Miss` 结构体 `args` 字段 |
| `mod.rs:219-222,228` | 缓存键构造中的 args 拆分 |
| `mod.rs:260-263` | 无缓存路径 `misses` 构造中的 args |
| `mod.rs:277-282` | 子请求组装中的 template_args 重打包 |
| `compose.rs:118,121-126` | `compose_embedding_texts` 的 `template_args` 参数与 `args_for` 闭包 |
| `compose.rs:21-63` | `render_template` 的 `args` 参数、`{1}+` 索引解析、越界报错——渲染器简化为仅替换 `{0}`，其余原样输出 |
| `embedding_cache.rs:32` | 注释中 template_args 描述 |

### 4.6 设置页 schema 与 i18n

| 位置 | 删除内容 |
|------|----------|
| `model_openai_config.rs:224-270` | `task_templates` array schema 整段（含 task 下拉 6 选项与 template 文本框的 `object_items` 定义，order 13 字段消失无碍，无需重排其余 order） |
| i18n 三语言 | `model-entry.fields.task_templates` 键组（zh-Hans.json:690-711，含 `task`/`template` 子键） |
| i18n 三语言 | `model-entry.tasks` 键组（zh-Hans.json:723-729）——仅被 task_templates 下拉引用，随之失去消费方 |

> i18n 保留键：`capabilities.reasoning`、`capabilities.output_dimensions`、`similarities.*`。

### 4.7 随之删除的测试用例

| 位置 | 用例 | 原因 |
|------|------|------|
| `compose.rs:234-249` | `user_template_overrides_builtin` | 用户模板层已删除 |
| `compose.rs:251-268` | `template_placeholder_out_of_range_rejected` | template_args 已删除 |
| `compose.rs:271-287` | `multi_placeholder_template_renders_in_order` | 同上 |
| `compose.rs:173-185` | `plain_text_task_gemma_fallback_passes_through` | 兜底已删除（改写为"无档案模型所有任务裸传"，见 §5.3） |
| `openai_compatible_provider.rs:393-407` | `embedding_rejects_task_type_without_capability` | TaskType 能力位已删除 |
| `model_profiles.rs:298-310` | `user_template_overrides_builtin` | 同上 |
| `model_profiles.rs:321-365` | `apply_profile_fills...` / `apply_profile_keeps...` / `apply_profile_unknown_model_noop` 三个用例 | `apply_profile_defaults` 已删除 |
| `model_openai_config.rs:444-453` | embedding enum 断言中 `"taskType"` | 改为 `["outputDimensions"]`（保留用例本体） |

## 5. 修改与新增清单

### 5.1 契约层 `crates/plugin-api/src/services/model/types.rs`

- `task_type` 字段（:220-222）：注释改为**必填**语义——"匹配模式，必填；序列化形状保留 Option 仅为容错，宿主入口对缺失/未知值返回 InvalidRequest"
- `ModelEmbeddingResponse`（:227）：文档注释删除"归一化"承诺，改为"与 input 一一对应的向量（是否归一化取决于 provider）"
- `EmbeddingCapability`（:56-66）：仅剩 `OutputDimensions`，枚举注释同步更新

### 5.2 宿主核心

**`compose.rs`**：

- 新增 `validate_task_type(task_type: Option<&str>) -> Result<SemanticTask, ModelError>`：`None`/空串 → 必填报错（错误消息见 §3.1）；未知 → 报错；命中 → 返回枚举。替代原 `validate_embedding_request`
- `compose_embedding_texts` 新签名：

```rust
pub(crate) fn compose_embedding_texts(
    input: &[String],
    task: SemanticTask,
    model_id: &str,
) -> Result<Vec<String>, ModelError>
```

  实现：`template_for(task, model_id)` 命中 → 渲染（仅 `{0}` 替换）；未命中（无档案或档案缺该任务）→ `Ok(input.to_vec())`
- `render_template` 简化：签名 `(template: &str, text: &str) -> String`，仅识别 `{0}`；其余字符（含非数字占位符与 `{1}`）原样保留
- `require_embedding_capability` 保留（OutputDimensions 门控仍在用）

**`model_profiles.rs`**：

- `template_for` 新签名：`pub fn template_for(task: SemanticTask, model_id: &str) -> Option<&'static str>`（返回档案模板；无档案/档案缺任务返回 `None`）
- 模块头注释重写为新规则（§3.2）
- `EmbeddingModelProfile` 仅剩 `id_prefix` + `task_templates` 两字段

**`mod.rs`**：

- `:190` 替换为 `let task = compose::validate_task_type(req.task_type.as_deref())?;`，解析出的 `SemanticTask` 传入后续 `compose_embedding_texts`
- `:19` import 行同步
- 缓存单条拆分逻辑保留（`task_type` 已参与缓存键 `embedding_cache.rs:219-221`，行为不变；`template_args` 相关行按 §4.5 删除）

**`openai_compatible_provider.rs` / `ollama_provider.rs`**：

- `embedding()` 中 `compose_embedding_texts` 调用改为新签名（openai :342-353、ollama :290-301），删除 capabilities/task_templates/template_args 传参
- 其余逻辑（dimensions 能力门控、请求构造）不变

### 5.3 新增测试

| 位置 | 用例 |
|------|------|
| `compose.rs` | `missing_task_type_rejected`：`None` 与空串均返回 `InvalidRequest` |
| `compose.rs` | `unknown_task_type_rejected`（保留自现有用例） |
| `compose.rs` | `unknown_model_all_tasks_pass_through`：无档案模型 + `retrieval_query`/`plain_text` 均裸传（替代被删的 gemma fallback 用例） |
| `model_profiles.rs` | `template_for_unknown_model_returns_none`（改写自现有用例，断言 `None`） |
| `model_profiles.rs` | `template_for_returns_none_when_profile_lacks_task`（可选，构造缺任务场景） |
| `mod.rs` | `embedding_requires_task_type`：入口校验集成测试 |

### 5.4 注释与文档同步（小项）

- `host_proxy.rs:203`、`plugin_handle.rs:377` 的 `model_embedding` 文档注释补一句"task_type 必填"
- `service.rs:31-35` / `provider.rs:41-45` 的 embedding 注释同步

## 6. 连带修复：cosine 真余弦（与本重构同链路，建议同批实施）

`mod.rs:368-374` 的 `cosine_similarity` 当前为裸点积，契约注释（已删除的"归一化"承诺）与实现矛盾，且 Ollama/兼容网关的 embedding 输出普遍未归一化，默认 `Cosine` 配置下排序数学错误。修复为：

```rust
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let (dot, na, nb) = a.iter().zip(b).fold((0f32, 0f32, 0f32), |(d, x, y), (u, v)| {
        (d + u * v, x + u * u, y + v * v)
    });
    let denom = na.sqrt() * nb.sqrt();
    if denom <= f32::EPSILON || !denom.is_finite() {
        return 0.0;
    }
    dot / denom
}
```

- `mod.rs:618-655` 的 cosine 测试当前全用单位向量（与 bug 同源自证），补充非单位向量用例（如 `[3.0, 4.0]` 与 `[4.0, 3.0]` 应为 `24/25`）
- `ModelSimilarity::Cosine` 的文档注释（types.rs:71-74）"归一化向量与点积排序等价"删除

## 7. 兼容性说明（按决策：不做旧代码兼容）

1. **旧配置**：已持久化设置 JSON 中的 `task_templates` 数组与 `taskType` 能力值——serde 默认忽略未知字段，`TaskTemplateItem`/字段删除后静默丢弃，无需迁移代码
2. **旧缓存**：`cache_namespace` 包含 models 配置序列化（openai :218-221、ollama :146-149），配置结构变化使 namespace 变化，旧 L1/L2 缓存条目整体失效，属一次性失效，可接受
3. **调用方**：当前 `task_type` 在全部非测试代码路径中均为 `None`（宿主内部无 embedding 消费方、translator 不用 embedding、无第三方插件使用），必填化无实际破坏面

## 8. 实施顺序建议

| 步骤 | 内容 | 说明 |
|------|------|------|
| 1 | 契约层 types.rs（§4.4、§4.5、§5.1） | 先定契约形状 |
| 2 | 宿主核心：settings → model_profiles → compose → mod → 两个 provider（§4.1-4.3、§5.2、§5.3） | 依赖契约层，逐层向内 |
| 3 | 配置组件 + i18n（§4.2 后两项、§4.6） | schema 驱动 UI，删字段即生效 |
| 4 | cosine 连带修复（§6） | 可独立 commit |

步骤 1-3 建议作为单个 commit（契约与实现不可分离地联动）；步骤 4 独立 commit。

## 9. 后续可选（不在本次范围）

- `ModelManager` 提供 `embed_query()` / `embed_documents()` 语义化便捷方法，内部固定传 `RetrievalQuery` / `RetrievalDocument`，进一步收敛调用方心智
- 档案库补录更多模型（bge、nomic-embed 等）提升有档案覆盖率——无档案模型走裸传后，补录是提升检索质量的唯一路径

## 10. 后续演进（实施后追加）：task_type 枚举强制

§3.1 的运行时白名单校验（`Option<String>` + `validate_task_type`）在首个第三方消费方（ai-search 插件）接入时被否——字符串让插件可以传任意值，宿主只能事后拒绝。改为**类型系统强制**：

- `SemanticTask` 枚举从宿主内部迁至 `plugin-api` 契约层（`types.rs`），带 `#[serde(rename)]` 序列化名
- `ModelEmbeddingRequest.task_type` 由 `Option<String>` 改为非 Option 的 `SemanticTask`；未知字符串在 serde 反序列化时直接失败（fail fast），不再进入宿主
- 宿主删除 `validate_task_type` / `parse_semantic_task`（枚举已保证合法），入口直接消费 `req.task_type`
- 序列化 JSON 形状不变（`taskType: "retrieval_document"`），旧插件若传合法值仍兼容；传非法值反序列化报错

| 文件 | 变更 |
|------|------|
| `plugin-api/types.rs` | 新增 `SemanticTask` 枚举 + `as_str()`；`task_type: SemanticTask` |
| `plugin-api/model/mod.rs`、`services/mod.rs` | re-export `SemanticTask` |
| `model_profiles.rs` | 删本地枚举定义，`use` 契约枚举 |
| `compose.rs` | 删 `parse_semantic_task`/`validate_task_type` 与相关测试 |
| `mod.rs`、两个 provider | 删入口校验调用；测试构造改枚举 |
| `ai-search` 插件 | `SemanticTask::RetrievalDocument` / `RetrievalQuery` 枚举传参 |

