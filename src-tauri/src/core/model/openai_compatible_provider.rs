//! 内置 OpenAI 兼容模型提供方：经 async-openai 调用任何 OpenAI 协议服务
//! （OpenAI / DeepSeek / Ollama /v1 等）。文本与向量模型清单由配置声明。

use super::compose::{compose_embedding_texts, require_embedding_capability};
use super::settings::{ModelEntryConfig, ModelOpenAiSettings};
use async_openai::config::OpenAIConfig;
use async_openai::types::chat::{
    ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestAssistantMessageContent,
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestSystemMessageContent, ChatCompletionRequestUserMessageArgs,
    ChatCompletionRequestUserMessageContent, CreateChatCompletionRequestArgs, FinishReason,
    ReasoningEffort,
};
use async_openai::types::embeddings::{CreateEmbeddingRequestArgs, EmbeddingInput};
use async_openai::Client;
use async_trait::async_trait;
use futures_util::StreamExt;
use std::pin::pin;
use tokio::sync::mpsc;
use zerolaunch_plugin_api::services::model::{
    ChatCapability, EmbeddingCapability, ModelChatMessage, ModelChatRequest, ModelChatResponse,
    ModelChatRole, ModelEmbeddingRequest, ModelEmbeddingResponse, ModelError, ModelInfo, ModelKind,
    ModelProvider, ModelSimilarity, ModelStreamChunk,
};

/// 提供方标识：与 model_id 的 `openai/` 前缀一致。
pub const PROVIDER_ID: &str = "openai";

/// 截取 model_id 的模型名部分（去掉 `provider/` 前缀）。
fn short_model(model_id: &str) -> &str {
    model_id.split_once('/').map(|(_, m)| m).unwrap_or(model_id)
}

/// 按模型名查找配置条目。
fn entry_for<'a>(
    settings: &'a ModelOpenAiSettings,
    model_id: &str,
) -> Option<&'a ModelEntryConfig> {
    let name = short_model(model_id);
    settings.models.iter().find(|entry| entry.name() == name)
}

/// 提取 chat 配置，缺少 chat 条目时返回 None。
fn chat_config(entry: Option<&ModelEntryConfig>) -> Option<&super::settings::ChatModelConfig> {
    match entry {
        Some(ModelEntryConfig::Chat { config, .. }) => Some(config),
        _ => None,
    }
}

/// 校验 reasoning 参数与模型声明能力一致。
fn ensure_reasoning_supported(
    entry: Option<&ModelEntryConfig>,
    reasoning_effort: &str,
) -> Result<(), ModelError> {
    if reasoning_effort != "none" {
        if let Some(config) = chat_config(entry) {
            if !config.capabilities.contains(&ChatCapability::Reasoning) {
                return Err(ModelError::InvalidRequest(
                    "模型未声明支持 reasoning 能力".to_string(),
                ));
            }
        }
    }
    Ok(())
}

/// 解析 chat 请求的最终参数：请求显式值优先，未设置时使用通用默认值。
fn effective_chat_params(
    req: &ModelChatRequest,
    entry: Option<&ModelEntryConfig>,
) -> (f32, u32, f32, String) {
    let defaults = chat_config(entry).cloned().unwrap_or_default();
    (
        req.temperature.unwrap_or(defaults.temperature),
        req.max_tokens.unwrap_or(defaults.max_tokens),
        defaults.top_p,
        req.reasoning_effort
            .clone()
            .unwrap_or(defaults.reasoning_effort),
    )
}

/// 将模型与请求的 chat 参数写入 OpenAI 请求构造器。
fn apply_chat_params(
    args: &mut CreateChatCompletionRequestArgs,
    req: &ModelChatRequest,
    entry: Option<&ModelEntryConfig>,
) -> Result<(), ModelError> {
    let (temperature, max_tokens, top_p, reasoning_effort) = effective_chat_params(req, entry);
    ensure_reasoning_supported(entry, &reasoning_effort)?;
    args.temperature(temperature)
        .max_completion_tokens(max_tokens)
        .top_p(top_p);
    let effort = match reasoning_effort.as_str() {
        "none" => ReasoningEffort::None,
        "low" => ReasoningEffort::Low,
        "medium" => ReasoningEffort::Medium,
        "high" => ReasoningEffort::High,
        other => {
            return Err(ModelError::InvalidRequest(format!(
                "无效的 reasoning_effort: {other}"
            )));
        }
    };
    args.reasoning_effort(effort);
    Ok(())
}

/// 按配置条目构建 chat 模型的 ModelInfo。
fn chat_model_info(name: String, capabilities: Vec<ChatCapability>) -> ModelInfo {
    ModelInfo {
        model_id: format!("{PROVIDER_ID}/{name}"),
        name: name.clone(),
        kind: ModelKind::Chat {
            context_window: None,
            max_output: None,
            supports_stream: true,
            capabilities,
        },
        provider: PROVIDER_ID.to_string(),
    }
}

/// 按配置条目构建 embedding 模型的 ModelInfo。
fn embedding_model_info(
    name: String,
    context_length: Option<u32>,
    dimensions: Option<u32>,
    similarity: ModelSimilarity,
    capabilities: Vec<EmbeddingCapability>,
) -> ModelInfo {
    ModelInfo {
        model_id: format!("{PROVIDER_ID}/{name}"),
        name,
        kind: ModelKind::Embedding {
            context_window: context_length,
            dimensions,
            similarity,
            capabilities,
        },
        provider: PROVIDER_ID.to_string(),
    }
}

/// 将统一消息转换为 OpenAI 协议消息。
fn to_openai_messages(
    messages: &[ModelChatMessage],
) -> Result<Vec<ChatCompletionRequestMessage>, ModelError> {
    messages
        .iter()
        .map(|m| match m.role {
            ModelChatRole::System => Ok(ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content(ChatCompletionRequestSystemMessageContent::Text(
                        m.content.clone(),
                    ))
                    .build()
                    .map_err(|e| ModelError::InvalidRequest(e.to_string()))?,
            )),
            ModelChatRole::User => Ok(ChatCompletionRequestMessage::User(
                ChatCompletionRequestUserMessageArgs::default()
                    .content(ChatCompletionRequestUserMessageContent::Text(
                        m.content.clone(),
                    ))
                    .build()
                    .map_err(|e| ModelError::InvalidRequest(e.to_string()))?,
            )),
            ModelChatRole::Assistant => Ok(ChatCompletionRequestMessage::Assistant(
                ChatCompletionRequestAssistantMessageArgs::default()
                    .content(ChatCompletionRequestAssistantMessageContent::Text(
                        m.content.clone(),
                    ))
                    .build()
                    .map_err(|e| ModelError::InvalidRequest(e.to_string()))?,
            )),
        })
        .collect()
}

/// 将 OpenAI 完成原因转换为统一字符串。
fn finish_reason_str(reason: FinishReason) -> String {
    match reason {
        FinishReason::Stop => "stop".to_string(),
        FinishReason::Length => "length".to_string(),
        FinishReason::ToolCalls => "tool_calls".to_string(),
        FinishReason::ContentFilter => "content_filter".to_string(),
        FinishReason::FunctionCall => "function_call".to_string(),
    }
}

/// OpenAI 兼容提供方：异步客户端按调用时构建（base_url/key 读配置），模型清单来自配置。
pub struct OpenAiCompatibleProvider {
    settings: ModelOpenAiSettings,
}

impl OpenAiCompatibleProvider {
    /// 创建提供方实例。
    pub fn new(settings: ModelOpenAiSettings) -> Self {
        Self { settings }
    }

    /// 按当前配置构建 OpenAI 兼容客户端。
    fn client(&self) -> Client<OpenAIConfig> {
        let config = OpenAIConfig::default()
            .with_api_base(&self.settings.base_url)
            .with_api_key(&self.settings.api_key);
        Client::with_config(config)
    }
}

/// 总超时：模型生成/embedding 单次调用上限（reqwest 无默认总超时，
/// 误配 base_url 或服务挂起时会永久阻塞；外层 timeout 兜底）。
const MODEL_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

#[async_trait]
impl ModelProvider for OpenAiCompatibleProvider {
    fn provider_id(&self) -> String {
        PROVIDER_ID.to_string()
    }
    /// 返回 endpoint 与模型配置组成的 embedding 缓存命名空间。
    fn cache_namespace(&self) -> String {
        // 仅含 provider + base_url：请求参数（模型/文本/任务/维度）已参与 compute_key，
        // 全量 models JSON 掺入会让无关配置改动（如 temperature）清空整个 embedding 缓存。
        format!("{}:{}", PROVIDER_ID, self.settings.base_url)
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ModelError> {
        let mut models = Vec::new();
        for entry in &self.settings.models {
            if entry.name().is_empty() {
                continue;
            }
            match entry {
                ModelEntryConfig::Chat { name, config } => {
                    models.push(chat_model_info(name.clone(), config.capabilities.clone()))
                }
                ModelEntryConfig::Embedding { name, config } => {
                    models.push(embedding_model_info(
                        name.clone(),
                        (config.context_length > 0).then_some(config.context_length),
                        None,
                        config.similarity,
                        config.capabilities.clone(),
                    ));
                }
            }
        }
        Ok(models)
    }

    async fn chat(&self, req: ModelChatRequest) -> Result<ModelChatResponse, ModelError> {
        let mut args = CreateChatCompletionRequestArgs::default();
        args.model(short_model(&req.model_id).to_string())
            .messages(to_openai_messages(&req.messages)?);
        let entry = entry_for(&self.settings, &req.model_id);
        apply_chat_params(&mut args, &req, entry)?;
        let build = args
            .build()
            .map_err(|e| ModelError::InvalidRequest(e.to_string()))?;

        let client = self.client();
        let resp = tokio::time::timeout(MODEL_CALL_TIMEOUT, client.chat().create(build))
            .await
            .map_err(|_| ModelError::Transport("模型调用超时".to_string()))?
            .map_err(|e| ModelError::Transport(e.to_string()))?;

        let content = resp
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();
        let usage = resp.usage.map(
            |u| zerolaunch_plugin_api::services::model::ModelTokenUsage {
                input_tokens: u.prompt_tokens,
                output_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
            },
        );
        Ok(ModelChatResponse {
            model_id: req.model_id,
            content,
            finish_reason: resp
                .choices
                .first()
                .and_then(|c| c.finish_reason)
                .map(finish_reason_str),
            usage,
        })
    }

    async fn stream_chat(
        &self,
        req: ModelChatRequest,
        tx: mpsc::Sender<ModelStreamChunk>,
    ) -> Result<(), ModelError> {
        let mut args = CreateChatCompletionRequestArgs::default();
        args.model(short_model(&req.model_id).to_string())
            .messages(to_openai_messages(&req.messages)?);
        let entry = entry_for(&self.settings, &req.model_id);
        apply_chat_params(&mut args, &req, entry)?;
        let build = args
            .build()
            .map_err(|e| ModelError::InvalidRequest(e.to_string()))?;
        let client = self.client();
        let mut stream = pin!(tokio::time::timeout(
            MODEL_CALL_TIMEOUT,
            client.chat().create_stream(build),
        )
        .await
        .map_err(|_| ModelError::Transport("模型调用超时".to_string()))?
        .map_err(|e| ModelError::Transport(e.to_string()))?);
        let mut finish_reason = None;
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(chunk) => {
                    if let Some(choice) = chunk.choices.first() {
                        if let Some(delta) = choice.delta.content.as_deref() {
                            tx.send(ModelStreamChunk::Delta(delta.to_string()))
                                .await
                                .map_err(|_| ModelError::Transport("流通道已关闭".to_string()))?;
                        }
                        finish_reason = choice
                            .finish_reason
                            .map(finish_reason_str)
                            .or(finish_reason);
                    }
                }
                Err(e) => {
                    let _ = tx.send(ModelStreamChunk::Error(e.to_string())).await;
                    return Err(ModelError::Transport(e.to_string()));
                }
            }
        }
        let _ = tx.send(ModelStreamChunk::Done { finish_reason }).await;
        Ok(())
    }

    async fn embedding(
        &self,
        req: ModelEmbeddingRequest,
    ) -> Result<ModelEmbeddingResponse, ModelError> {
        let entry = entry_for(&self.settings, &req.model_id);
        let embedding_config = entry.and_then(|value| match value {
            ModelEntryConfig::Embedding { config, .. } => Some(config),
            ModelEntryConfig::Chat { .. } => None,
        });
        let texts = compose_embedding_texts(
            &req.input,
            req.task_type,
            &req.model_id,
            req.template_args.as_deref(),
        )?;
        let mut args = CreateEmbeddingRequestArgs::default();
        args.model(short_model(&req.model_id).to_string())
            .input(EmbeddingInput::StringArray(texts));
        if let Some(dimensions) = req.dimensions {
            if let Some(config) = embedding_config {
                require_embedding_capability(
                    &config.capabilities,
                    EmbeddingCapability::OutputDimensions,
                    "outputDimensions",
                )?;
            }
            args.dimensions(dimensions);
        }
        let build = args
            .build()
            .map_err(|e| ModelError::InvalidRequest(e.to_string()))?;
        let client = self.client();
        let resp = tokio::time::timeout(MODEL_CALL_TIMEOUT, client.embeddings().create(build))
            .await
            .map_err(|_| ModelError::Transport("模型调用超时".to_string()))?
            .map_err(|e| ModelError::Transport(e.to_string()))?;
        let vectors: Vec<Vec<f32>> = resp.data.into_iter().map(|item| item.embedding).collect();
        let dimensions = vectors
            .first()
            .map(|vector| vector.len())
            .unwrap_or_default();
        Ok(ModelEmbeddingResponse {
            model_id: req.model_id,
            dimensions: dimensions as u32,
            vectors,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zerolaunch_plugin_api::services::model::SemanticTask;

    #[tokio::test]
    async fn chat_rejects_invalid_reasoning_effort() {
        let provider = OpenAiCompatibleProvider::new(Default::default());
        let err = provider
            .chat(ModelChatRequest {
                model_id: "openai/some-model".to_string(),
                messages: vec![],
                temperature: None,
                max_tokens: None,
                reasoning_effort: Some("extreme".to_string()),
            })
            .await
            .unwrap_err();
        assert!(matches!(err, ModelError::InvalidRequest(_)));
    }

    #[tokio::test]
    async fn list_models_partitions_by_entry_kind_and_skips_empty_name() {
        let mut settings = ModelOpenAiSettings::default();
        settings.models = vec![
            ModelEntryConfig::chat("gpt-4o-mini"),
            ModelEntryConfig::Embedding {
                name: "text-embedding-3-small".to_string(),
                config: super::super::settings::EmbeddingModelConfig {
                    dimensions: 1536,
                    context_length: 8192,
                    similarity: ModelSimilarity::DotProduct,
                    capabilities: vec![EmbeddingCapability::OutputDimensions],
                },
            },
            ModelEntryConfig::Chat {
                name: String::new(),
                config: Default::default(),
            },
        ];
        let models = OpenAiCompatibleProvider::new(settings)
            .list_models()
            .await
            .unwrap();
        assert_eq!(models.len(), 2);
        if let ModelKind::Embedding {
            context_window,
            dimensions,
            similarity,
            capabilities,
        } = &models[1].kind
        {
            assert_eq!(*context_window, Some(8192));
            assert_eq!(*dimensions, None);
            assert_eq!(*similarity, ModelSimilarity::DotProduct);
            assert_eq!(capabilities.len(), 1);
        } else {
            panic!("expected embedding model");
        }
        assert_eq!(models[0].model_id, "openai/gpt-4o-mini");
        assert!(matches!(models[0].kind, ModelKind::Chat { .. }));
        assert_eq!(models[1].model_id, "openai/text-embedding-3-small");
        assert!(matches!(models[1].kind, ModelKind::Embedding { .. }));
    }

    /// 未声明可选能力时拒绝对应的 chat 与 embedding 请求。
    #[tokio::test]
    async fn reject_requests_for_unlisted_capabilities() {
        let chat_entry = ModelEntryConfig::Chat {
            name: "chat-model".to_string(),
            config: super::super::settings::ChatModelConfig {
                capabilities: Vec::new(),
                ..Default::default()
            },
        };
        let chat_settings = ModelOpenAiSettings {
            models: vec![chat_entry],
            ..Default::default()
        };
        let chat_err = OpenAiCompatibleProvider::new(chat_settings)
            .chat(ModelChatRequest {
                model_id: "openai/chat-model".to_string(),
                messages: vec![],
                temperature: None,
                max_tokens: None,
                reasoning_effort: Some("low".to_string()),
            })
            .await
            .unwrap_err();
        assert!(matches!(chat_err, ModelError::InvalidRequest(_)));

        let embedding_entry = ModelEntryConfig::Embedding {
            name: "embedding-model".to_string(),
            config: super::super::settings::EmbeddingModelConfig {
                capabilities: Vec::new(),
                ..Default::default()
            },
        };
        let embedding_settings = ModelOpenAiSettings {
            models: vec![embedding_entry],
            ..Default::default()
        };
        let embedding_err = OpenAiCompatibleProvider::new(embedding_settings)
            .embedding(ModelEmbeddingRequest {
                model_id: "openai/embedding-model".to_string(),
                input: vec!["text".to_string()],
                template_args: None,
                task_type: SemanticTask::RetrievalQuery,
                dimensions: Some(128),
            })
            .await
            .unwrap_err();
        assert!(matches!(embedding_err, ModelError::InvalidRequest(_)));
    }

    #[test]
    fn effective_chat_params_prefer_request_over_entry() {
        let entry = ModelEntryConfig::Chat {
            name: "m1".to_string(),
            config: super::super::settings::ChatModelConfig {
                temperature: 0.2,
                max_tokens: 64,
                top_p: 0.5,
                ..Default::default()
            },
        };
        let req = ModelChatRequest {
            model_id: "openai/m1".to_string(),
            messages: vec![],
            temperature: Some(0.9),
            max_tokens: None,
            reasoning_effort: None,
        };
        let (t, m, p, _e) = effective_chat_params(&req, Some(&entry));
        assert_eq!(t, 0.9);
        assert_eq!(m, 64);
        assert_eq!(p, 0.5);
    }
}
