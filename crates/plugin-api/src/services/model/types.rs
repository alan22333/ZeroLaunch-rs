//! 模型服务共享类型：模型元数据、请求/响应载荷与统一错误。

use serde::{Deserialize, Serialize};

/// 模型种类：chat（文本生成）/ embedding（文本向量化）。
///
/// 变体携带各自专属的规格配置与能力清单，由类型系统强制按种类归类；
/// 经 `kind` 匹配后只能访问该种类的字段，消除无关字段的认知负担。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ModelKind {
    /// chat（文本生成）——由模型提供方声明，消费者按此路由 chat 请求。
    #[serde(rename = "chat")]
    Chat {
        /// chat：上下文窗口大小（可选；None 表示提供方未声明）。
        #[serde(rename = "contextWindow")]
        context_window: Option<u32>,
        /// chat：单次最大输出 token（可选；None 表示提供方未声明）。
        #[serde(rename = "maxOutput")]
        max_output: Option<u32>,
        /// 是否支持流式输出（仅 chat 请求有流式概念）。
        #[serde(rename = "supportsStream")]
        supports_stream: bool,
        /// 支持的请求级可选能力清单（provider 如实声明，消费者按能力传参）。
        #[serde(rename = "capabilities")]
        capabilities: Vec<ChatCapability>,
    },
    /// embedding（文本向量化）——由模型提供方声明，消费者按此路由 embedding 请求。
    #[serde(rename = "embedding")]
    Embedding {
        /// embedding：模型可接受的最大输入上下文长度（可选；None 表示提供方未声明）。
        #[serde(rename = "contextWindow")]
        context_window: Option<u32>,
        /// embedding：向量维度（可选；None 表示提供方未声明，用于校验向量一致性）。
        #[serde(rename = "dimensions")]
        dimensions: Option<u32>,
        /// embedding 模型推荐的相似度计算方式（默认 Cosine）。
        #[serde(rename = "similarity")]
        #[serde(default)]
        similarity: ModelSimilarity,
        /// 支持的请求级可选能力清单（provider 如实声明，消费者按能力传参）。
        #[serde(rename = "capabilities")]
        capabilities: Vec<EmbeddingCapability>,
    },
}

/// chat 模型支持的请求级可选能力——provider 在 list_models 如实声明，
/// 消费者按能力传参；能力不匹配的请求返回 `ModelError::InvalidRequest`，不静默忽略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatCapability {
    /// 支持深度思考档位（`reasoning_effort`: none/low/medium/high）。
    #[serde(rename = "reasoning")]
    Reasoning,
}

/// embedding 模型支持的请求级可选能力——provider 在 list_models 如实声明，
/// 消费者按能力传参；能力不匹配的请求返回 `ModelError::InvalidRequest`，不静默忽略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmbeddingCapability {
    /// 支持标题+正文输入（`titles` 与 `input` 一一对应）。
    #[serde(rename = "title")]
    Title,
    /// 支持匹配模式（`task_type`: retrieval_document / retrieval_query 等）。
    #[serde(rename = "taskType")]
    TaskType,
    /// 支持输出维度裁剪（`dimensions`，matryoshka 类模型）。
    #[serde(rename = "outputDimensions")]
    OutputDimensions,
}

/// embedding 模型推荐的 相似度计算方式（消费者按此选择公式；仅 embedding 模型有意义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ModelSimilarity {
    /// 余弦相似度；稠密 embedding 惯例（归一化向量与点积排序等价）。
    #[default]
    #[serde(rename = "cosine")]
    Cosine,
    /// 点积；未归一化向量或稀疏向量的常见选择。
    #[serde(rename = "dotProduct")]
    DotProduct,
    /// 欧氏距离；以负距离作为相似度。
    #[serde(rename = "euclidean")]
    Euclidean,
}

/// 模型元数据——提供方模型清单中的一条。
///
/// 由提供方 list_models 产出，经 host/model.list 下发消费者；
/// 种类专属配置收拢在 `kind` 变体内，序列化时平铺到顶层。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// 全局唯一模型 id：`{provider}/{model}`，如 `openai/gpt-4o-mini`、`ollama/qwen3:8b`。
    /// 消费者按此 id 调用，宿主按前缀路由到提供方。
    #[serde(rename = "modelId")]
    pub model_id: String,
    /// 显示名（如 "Qwen3 8B"）。
    #[serde(rename = "name")]
    pub name: String,
    /// 模型种类及种类专属规格配置。
    #[serde(flatten)]
    pub kind: ModelKind,
    /// 提供方标识（"openai" / "ollama" / 插件 id，与 model_id 前缀一致）。
    #[serde(rename = "provider")]
    pub provider: String,
}

/// 模型调用统一错误。
#[derive(Debug, Clone, thiserror::Error)]
pub enum ModelError {
    /// 提供方不支持该操作（如 ONNX 后端无 chat 能力）。
    #[error("模型提供方不支持该操作")]
    NotSupported,
    /// model_id 不存在于任何提供方。
    #[error("模型不存在: {0}")]
    ModelNotFound(String),
    /// 提供方不可达或未就绪（如 Ollama 未启动、模型未下载）。
    #[error("模型提供方不可用: {0}")]
    ProviderUnavailable(String),
    /// 请求载荷非法。
    #[error("请求参数非法: {0}")]
    InvalidRequest(String),
    /// 网络/传输失败。
    #[error("模型传输失败: {0}")]
    Transport(String),
}

/// 对话消息角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelChatRole {
    /// 系统提示消息，由 provider 作为 system 角色发送。
    #[serde(rename = "system")]
    System,
    /// 用户消息，由 provider 作为 user 角色发送。
    #[serde(rename = "user")]
    User,
    /// 助手历史消息，由 provider 作为 assistant 角色发送。
    #[serde(rename = "assistant")]
    Assistant,
}

impl Default for ModelChatRole {
    /// 返回缺失消息角色时使用的用户角色。
    fn default() -> Self {
        Self::User
    }
}

/// 对话消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelChatMessage {
    /// 消息角色，必须是 system、user 或 assistant。
    #[serde(rename = "role", default)]
    pub role: ModelChatRole,
    /// 消息文本；允许为空，由 provider 负责转换。
    #[serde(rename = "content", default)]
    pub content: String,
}

/// chat 请求：model_id 指定模型，宿主按前缀路由到提供方。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelChatRequest {
    /// 当前模型清单中的全局 model id；缺失或空值由宿主入口拒绝。
    #[serde(rename = "modelId", default)]
    pub model_id: String,
    /// 按顺序排列的对话消息；缺失时使用空列表并由 provider 处理。
    #[serde(rename = "messages", default)]
    pub messages: Vec<ModelChatMessage>,
    /// 请求级温度覆盖值；None 表示使用模型配置默认值。
    #[serde(rename = "temperature", default)]
    pub temperature: Option<f32>,
    /// 请求级最大输出 token 覆盖值；None 表示使用模型配置默认值。
    #[serde(rename = "maxTokens", default)]
    pub max_tokens: Option<u32>,
    /// 深度思考档位；None 表示使用模型配置默认值。
    #[serde(rename = "reasoningEffort", default)]
    pub reasoning_effort: Option<String>,
}

/// token 用量统计（可选返回）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelTokenUsage {
    /// 输入 token 数量；未知时为 0。
    #[serde(rename = "inputTokens", default)]
    pub input_tokens: u32,
    /// 输出 token 数量；未知时为 0。
    #[serde(rename = "outputTokens", default)]
    pub output_tokens: u32,
    /// 输入与输出 token 总数；未知时为 0。
    #[serde(rename = "totalTokens", default)]
    pub total_tokens: u32,
}

/// chat 响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelChatResponse {
    /// 实际响应对应的全局 model id。
    #[serde(rename = "modelId", default)]
    pub model_id: String,
    /// 模型生成的文本内容；无文本时为空字符串。
    #[serde(rename = "content", default)]
    pub content: String,
    /// provider 返回的结束原因；None 表示未提供。
    #[serde(rename = "finishReason", default)]
    pub finish_reason: Option<String>,
    /// provider 返回的 token 用量；None 表示未提供。
    #[serde(rename = "usage", default)]
    pub usage: Option<ModelTokenUsage>,
}

/// embedding 请求：批量向量化（输入列表，一一对应返回）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEmbeddingRequest {
    /// 当前模型清单中的 embedding model id；缺失或空值由宿主入口拒绝。
    #[serde(rename = "modelId", default)]
    pub model_id: String,
    /// 待向量化文本列表；缺失时使用空列表。
    #[serde(rename = "input", default)]
    pub input: Vec<String>,
    /// 与 input 一一对应的文档标题；None 表示不携带标题。
    #[serde(rename = "titles", default)]
    pub titles: Option<Vec<String>>,
    /// 匹配模式；None 表示不指定任务类型。
    #[serde(rename = "taskType", default)]
    pub task_type: Option<String>,
    /// 输出维度裁剪；None 表示使用模型原生维度。
    #[serde(rename = "dimensions", default)]
    pub dimensions: Option<u32>,
}
/// embedding 响应：与 input 一一对应的归一化向量。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEmbeddingResponse {
    /// 实际响应对应的 embedding model id。
    #[serde(rename = "modelId", default)]
    pub model_id: String,
    /// 每个向量的维度；无向量时为 0。
    #[serde(rename = "dimensions", default)]
    pub dimensions: u32,
    /// 与请求 input 一一对应的向量列表；缺失时使用空列表。
    #[serde(rename = "vectors", default)]
    pub vectors: Vec<Vec<f32>>,
}

/// embedding 相似度请求：一个查询向量与多个目标向量两两计算相似度。
///
/// 向量必须同维度（由宿主校验）；维度不一致返回 InvalidRequest。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSimilarityRequest {
    /// 用于计算相似度的 embedding model id（仅用于取模型相似度公式）。
    #[serde(rename = "modelId", default)]
    pub model_id: String,
    /// 查询向量（单个）。
    #[serde(rename = "query", default)]
    pub query: Vec<f32>,
    /// 目标向量列表（与 query 两两计算，结果一一对应）。
    #[serde(rename = "targets", default)]
    pub targets: Vec<Vec<f32>>,
}

/// embedding 相似度响应：查询向量与每个目标向量的相似度（顺序与 targets 一致）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSimilarityResponse {
    /// 实际响应对应的 embedding model id。
    #[serde(rename = "modelId", default)]
    pub model_id: String,
    /// 与 targets 一一对应的相似度列表。
    #[serde(rename = "similarities", default)]
    pub similarities: Vec<f32>,
}

/// 流式输出分块（宿主内部传播，不跨插件协议）。
#[derive(Debug, Clone)]
pub enum ModelStreamChunk {
    /// 增量文本。
    Delta(String),
    /// 流结束（含结束原因）。
    Done { finish_reason: Option<String> },
    /// 流中途出错。
    Error(String),
}
