//! 模型提供方接口：内置后端（OpenAI 兼容 / Ollama）与第三方插件后端实现同一接口。

use async_trait::async_trait;
use tokio::sync::mpsc;

use super::types::{
    ModelChatRequest, ModelChatResponse, ModelEmbeddingRequest, ModelEmbeddingResponse, ModelError,
    ModelInfo, ModelStreamChunk,
};

/// 模型提供方：声明模型清单并提供模型调用能力。
///
/// 宿主内置提供方（OpenAI 兼容 / Ollama）与第三方插件后端（插件注册为
/// Provider 组件）均实现本接口；宿主 `ModelRegistry` 聚合清单并按
/// `provider_id` 前缀路由调用。提供方只实现自己支持的模型种类，
/// 不支持的调用返回 `ModelError::NotSupported`。
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// 提供方标识（如 "openai"、"ollama"、插件 id）。
    /// 与 model_id 的 `{provider}/` 前缀一致，宿主按此前缀路由。
    fn provider_id(&self) -> String;
    /// 返回影响 embedding 结果的 provider 配置命名空间，供宿主缓存隔离。
    /// 默认使用 provider_id；配置化 provider 应覆盖并包含 endpoint/模型配置版本。
    fn cache_namespace(&self) -> String {
        self.provider_id()
    }

    /// 当前支持的模型清单（可能随环境/配置变化）。
    async fn list_models(&self) -> Result<Vec<ModelInfo>, ModelError>;

    /// 调用文本生成模型；仅支持 chat 模型的提供方实现。
    async fn chat(&self, req: ModelChatRequest) -> Result<ModelChatResponse, ModelError>;

    /// 调用流式文本生成模型：增量经 tx 推送，完成后关闭通道。
    async fn stream_chat(
        &self,
        req: ModelChatRequest,
        tx: mpsc::Sender<ModelStreamChunk>,
    ) -> Result<(), ModelError>;

    /// 调用文本向量化模型；仅支持 embedding 模型的提供方实现（task_type 必填）。
    async fn embedding(
        &self,
        req: ModelEmbeddingRequest,
    ) -> Result<ModelEmbeddingResponse, ModelError>;

    /// 提供方可用性探测（如 Ollama 未启动）；默认恒可用。
    async fn is_available(&self) -> bool {
        true
    }
}
