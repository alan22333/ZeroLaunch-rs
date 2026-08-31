use async_trait::async_trait;
use tokio::sync::mpsc;

use super::types::{
    ModelChatRequest, ModelChatResponse, ModelEmbeddingRequest, ModelEmbeddingResponse, ModelError,
    ModelInfo, ModelSimilarityRequest, ModelSimilarityResponse, ModelStreamChunk,
};

/// 宿主统一模型服务：消费者（内置组件经 PluginHandle、第三方插件经 host/model.*）
/// 通过本接口获取模型清单、按 model_id 调用模型。
///
/// 实现（宿主 ModelManager）负责聚合各提供方清单并按 model_id 前缀路由。
#[async_trait]
pub trait ModelService: Send + Sync {
    /// 全网模型清单（聚合缓存）。
    fn list_models(&self) -> Vec<ModelInfo>;

    /// 按 model_id 查询模型信息。
    fn model_info(&self, model_id: &str) -> Option<ModelInfo>;

    /// 文本生成。
    async fn chat(&self, req: ModelChatRequest) -> Result<ModelChatResponse, ModelError>;

    /// 流式文本生成：增量经 tx 推送，完成后关闭通道。
    async fn stream_chat(
        &self,
        req: ModelChatRequest,
        tx: mpsc::Sender<ModelStreamChunk>,
    ) -> Result<(), ModelError>;

    /// 文本向量化（task_type 必填，宿主对缺失/未知值返回 InvalidRequest）。
    async fn embedding(
        &self,
        req: ModelEmbeddingRequest,
    ) -> Result<ModelEmbeddingResponse, ModelError>;

    /// 查询向量与多个目标向量的相似度（按模型元数据公式计算，并行加速）。
    async fn similarity(
        &self,
        req: ModelSimilarityRequest,
    ) -> Result<ModelSimilarityResponse, ModelError>;
}
