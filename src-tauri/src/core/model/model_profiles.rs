//! 内置 embedding 模型档案库：按模型 id 前缀匹配模板规则。
//!
//! 语义任务（`SemanticTask`）是跨模型统一的抽象——插件只声明"我要做检索查询"，
//! 宿主按模型档案把语义任务翻译成该模型的输入模板。不同模型对同一任务的叫法
//! 不同（如 gemma 用 `task: search result | query: `，qwen3 用 `Instruct: ...\nQuery:`），
//! 统一在此层收敛，第三方插件无需感知各模型差异。
//!
//! 模板使用 `{0}`/`{1}`/`{2}` 位置占位符：`{0}` 固定为 input 文本，`{1}` 起为
//! 请求携带的 template_args（按顺序对应）。渲染器只做位置替换，不关心占位符名，
//! 参数顺序与个数由调用方保证（模型模板需要几个变量，请求就传几个）。
//!
//! 模板解析优先级：用户配置 → 内置档案 → gemma 兜底（`gemma_fallback_template`）。
//! 无档案模型（自定义/未收录）且 task_type 非空时使用 gemma 中性前缀模板，
//! 仅 task_type 为空时原样透传 input。所有模板仅引用 `{0}`（input 文本），
//! 不依赖调用方传 template_args。

use super::settings::ModelEntryConfig;
use serde::{Deserialize, Serialize};
use zerolaunch_plugin_api::services::model::EmbeddingCapability;

/// 语义任务类型：跨模型统一的任务抽象。
///
/// 序列化名为宿主 IPC 契约中的 `task_type` 字符串（`ModelEmbeddingRequest.task_type`），
/// 插件按此传参；宿主负责翻译成具体模型的输入模板。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SemanticTask {
    /// 检索文档（候选侧；多数模型裸传或加标题模板）。
    #[serde(rename = "retrieval_document")]
    RetrievalDocument,
    /// 检索查询（查询侧；多数模型加任务前缀）。
    #[serde(rename = "retrieval_query")]
    RetrievalQuery,
    /// 句子/文档语义相似度。
    #[serde(rename = "semantic_similarity")]
    SemanticSimilarity,
    /// 文本分类。
    #[serde(rename = "classification")]
    Classification,
    /// 文本聚类。
    #[serde(rename = "clustering")]
    Clustering,
    /// 裸文本直传（无任务模板）：输入原样透传，不套任何前缀。
    ///
    /// 用于模板反而干扰语义的模型（如部分 qwen3-embedding 对泛化查询
    /// 裸文本语义分更准）；等效于不带 task_type，但允许按任务显式配置。
    #[serde(rename = "plain_text")]
    PlainText,
}

impl SemanticTask {
    /// 所有语义任务（按固定顺序；设置页模板编辑区按此列出）。
    pub const ALL: [SemanticTask; 6] = [
        SemanticTask::RetrievalDocument,
        SemanticTask::RetrievalQuery,
        SemanticTask::SemanticSimilarity,
        SemanticTask::Classification,
        SemanticTask::Clustering,
        SemanticTask::PlainText,
    ];

    /// 语义任务的序列化名（与 serde rename 一致）。
    pub fn as_str(self) -> &'static str {
        match self {
            SemanticTask::RetrievalDocument => "retrieval_document",
            SemanticTask::RetrievalQuery => "retrieval_query",
            SemanticTask::SemanticSimilarity => "semantic_similarity",
            SemanticTask::Classification => "classification",
            SemanticTask::Clustering => "clustering",
            SemanticTask::PlainText => "plain_text",
        }
    }
}

/// 内置 embedding 模型档案：按模型 id 前缀匹配。
///
/// `capabilities` 是自动勾选的能力（如 qwen3 声明支持 taskType）；
/// `task_templates` 是语义任务 → 输入模板的映射（`{0}` = input 文本，`{1}`+ =
/// template_args 顺序填充）。
pub struct EmbeddingModelProfile {
    /// 模型 id 前缀（匹配 `{provider}/{name}` 中的 name 部分，如 "qwen3-embedding"）。
    pub id_prefix: &'static str,
    /// 自动勾选的 embedding 能力（模型实际支持的能力）。
    pub capabilities: &'static [EmbeddingCapability],
    /// 语义任务 → 输入模板。
    pub task_templates: &'static [(SemanticTask, &'static str)],
}

/// 无档案模型的 gemma 回退模板（Google EmbeddingGemma 官方模板）。
///
/// 查询侧 `task: {task} | query: {0}`，文档侧 `title: none | text: {0}`
/// （sentence-transformers 官方 prompts：`question: task: search result | query: `、
/// `passage_text: title: none | text: `；文档侧 title 恒为字面量 none，不取模板参数）。
/// 同时作为 gemma-embedding 档案的模板表（单一定义，避免两份重复）。
const GEMMA_FALLBACK: &[(SemanticTask, &str)] = &[
    (SemanticTask::RetrievalDocument, "title: none | text: {0}"),
    (
        SemanticTask::RetrievalQuery,
        "task: search result | query: {0}",
    ),
    (
        SemanticTask::SemanticSimilarity,
        "task: sentence similarity | query: {0}",
    ),
    (
        SemanticTask::Classification,
        "task: classification | query: {0}",
    ),
    (SemanticTask::Clustering, "task: clustering | query: {0}"),
    (SemanticTask::PlainText, "{0}"),
];

/// 内置模型档案表。
///
/// 当前收录 qwen3-embedding（Instruct 模板）与 gemma-embedding（title+text 双变量模板）。
/// 模板遵循官方文档：
/// - qwen3：查询侧 `Instruct: {task}\nQuery:{text}`（`{0}` = text），文档侧裸传
///   （官方 README 明确 "No need to add instruction for retrieval documents"）。
/// - gemma：查询侧 `task: {task} | query: {text}`，文档侧 `title: none | text: {text}`
///   （文档侧 title 为官方固定字面量 none，不引用模板参数）。
const PROFILES: &[EmbeddingModelProfile] = &[
    EmbeddingModelProfile {
        id_prefix: "qwen3-embedding",
        capabilities: &[EmbeddingCapability::TaskType],
        task_templates: &[
            (SemanticTask::RetrievalDocument, "{0}"),
            (
                SemanticTask::RetrievalQuery,
                "Instruct: Given a web search query, retrieve relevant passages that answer the query\nQuery:{0}",
            ),
            (
                SemanticTask::SemanticSimilarity,
                "Instruct: Given two sentences, determine how similar they are to each other\nQuery:{0}",
            ),
            (
                SemanticTask::Classification,
                "Instruct: Given a text, classify it into the appropriate category\nQuery:{0}",
            ),
            (
                SemanticTask::Clustering,
                "Instruct: Given a text, determine the cluster it belongs to\nQuery:{0}",
            ),
            (
                SemanticTask::PlainText,
                "{0}",
            ),
        ],
    },
    EmbeddingModelProfile {
        id_prefix: "gemma-embedding",
        capabilities: &[EmbeddingCapability::TaskType],
        task_templates: GEMMA_FALLBACK,
    },
];

/// 按模型 id 查内置档案。
///
/// 匹配规则：取模型 id 中 provider 前缀之后的 name 部分（如 `ollama/qwen3-embedding:0.6b`
/// → `qwen3-embedding:0.6b`），与档案 `id_prefix` 前缀匹配。
pub fn profile_for(model_id: &str) -> Option<&'static EmbeddingModelProfile> {
    let name = model_id.split('/').next_back().unwrap_or(model_id);
    PROFILES.iter().find(|p| name.starts_with(p.id_prefix))
}

/// 语义任务 → 模板：优先用户配置，否则内置档案，否则 None（裸传）。
///
/// `user_templates` 为用户在模型配置中覆盖的映射（key 为语义任务序列化名）。
pub fn template_for(
    task: SemanticTask,
    user_templates: &[super::settings::TaskTemplateItem],
    model_id: &str,
) -> Option<String> {
    if let Some(item) = user_templates.iter().find(|t| t.task == task.as_str()) {
        return Some(item.template.clone());
    }
    profile_for(model_id).and_then(|p| {
        p.task_templates
            .iter()
            .find(|(semantic, _)| *semantic == task)
            .map(|(_, template)| template.to_string())
    })
}

/// 无档案模型的任务模板兜底（与 gemma-embedding 档案共用同一张表）。
pub fn gemma_fallback_template(task: SemanticTask) -> Option<&'static str> {
    GEMMA_FALLBACK
        .iter()
        .find(|(t, _)| *t == task)
        .map(|(_, template)| *template)
}

/// 按模型 id 取内置任务模板（语义任务 → 模板；无档案返回空映射）。
pub fn auto_task_templates(model_id: &str) -> Vec<super::settings::TaskTemplateItem> {
    profile_for(model_id)
        .map(|p| {
            p.task_templates
                .iter()
                .map(|(task, template)| super::settings::TaskTemplateItem {
                    task: task.as_str().to_string(),
                    template: template.to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 应用内置档案自动填充到单个 embedding 模型配置。
///
/// 仅填充"用户未显式配置"的字段：能力为空或等于默认值时并入档案能力；
/// task_templates 为空时填档案模板。用户手动改过的配置不被覆盖。
pub fn apply_profile_defaults(
    model_id: &str,
    capabilities: &mut Vec<EmbeddingCapability>,
    task_templates: &mut Vec<super::settings::TaskTemplateItem>,
) {
    let Some(profile) = profile_for(model_id) else {
        return;
    };
    // 能力：默认值（仅 outputDimensions）或空时并入档案能力（去重）。
    let is_default_caps = capabilities.is_empty()
        || capabilities.as_slice() == [EmbeddingCapability::OutputDimensions];
    if is_default_caps {
        let mut merged = profile.capabilities.to_vec();
        for cap in capabilities.iter() {
            if !merged.contains(cap) {
                merged.push(*cap);
            }
        }
        *capabilities = merged;
    }
    // 模板：为空时填档案模板。
    if task_templates.is_empty() {
        *task_templates = auto_task_templates(model_id);
    }
}

/// 遍历模型条目清单，对匹配内置档案的 embedding 条目应用自动填充。
///
/// 供模型配置组件 apply_settings 调用（用户保存配置时自动勾选能力并填充模板）。
pub fn apply_profiles_to_entries(entries: &mut [ModelEntryConfig]) {
    for entry in entries.iter_mut() {
        if let ModelEntryConfig::Embedding { name, config } = entry {
            let model_id = name.clone();
            apply_profile_defaults(
                &model_id,
                &mut config.capabilities,
                &mut config.task_templates,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_matches_qwen3_with_provider_prefix() {
        let p = profile_for("ollama/qwen3-embedding:0.6b").expect("qwen3 应有档案");
        assert_eq!(p.id_prefix, "qwen3-embedding");
    }

    #[test]
    fn profile_matches_gemma() {
        let p = profile_for("ollama/gemma-embedding:300m").expect("gemma 应有档案");
        assert_eq!(p.id_prefix, "gemma-embedding");
    }

    #[test]
    fn profile_unknown_returns_none() {
        assert!(profile_for("ollama/unknown-model").is_none());
        assert!(profile_for("openai/text-embedding-3-large").is_none());
    }

    #[test]
    fn qwen3_retrieval_query_template_is_instruct() {
        let t = template_for(
            SemanticTask::RetrievalQuery,
            &[],
            "ollama/qwen3-embedding:0.6b",
        )
        .expect("qwen3 retrieval_query 应有模板");
        assert!(t.starts_with("Instruct: "));
        assert!(t.contains("{0}"));
    }

    #[test]
    fn gemma_document_template_uses_official_title_none() {
        let t = template_for(
            SemanticTask::RetrievalDocument,
            &[],
            "ollama/gemma-embedding:300m",
        )
        .expect("gemma retrieval_document 应有模板");
        assert_eq!(t, "title: none | text: {0}");
    }

    #[test]
    fn user_template_overrides_builtin() {
        let user = vec![super::super::settings::TaskTemplateItem {
            task: "retrieval_query".to_string(),
            template: "自定义 {0}".to_string(),
        }];
        let t = template_for(
            SemanticTask::RetrievalQuery,
            &user,
            "ollama/qwen3-embedding:0.6b",
        )
        .unwrap();
        assert_eq!(t, "自定义 {0}");
    }

    #[test]
    fn unknown_model_without_user_template_returns_none() {
        assert_eq!(
            template_for(SemanticTask::RetrievalQuery, &[], "ollama/unknown-model",),
            None
        );
    }

    #[test]
    fn apply_profile_fills_default_capabilities_and_templates() {
        let mut caps = vec![EmbeddingCapability::OutputDimensions];
        let mut templates = Vec::new();
        apply_profile_defaults("ollama/qwen3-embedding:0.6b", &mut caps, &mut templates);
        assert!(caps.contains(&EmbeddingCapability::TaskType));
        assert_eq!(templates.len(), 6);
        assert!(
            templates
                .iter()
                .find(|t| t.task == "retrieval_query")
                .unwrap()
                .template
                .starts_with("Instruct: "),
            "qwen3 模板应为 Instruct 风格"
        );
    }

    #[test]
    fn apply_profile_keeps_user_custom_capabilities() {
        // 用户显式配置仅 title：不被档案覆盖。
        let mut caps = vec![EmbeddingCapability::TaskType];
        let mut templates = Vec::new();
        apply_profile_defaults("ollama/qwen3-embedding:0.6b", &mut caps, &mut templates);
        assert_eq!(caps, vec![EmbeddingCapability::TaskType]);
    }

    #[test]
    fn apply_profile_keeps_user_templates() {
        let mut caps = vec![EmbeddingCapability::OutputDimensions];
        let mut templates = vec![super::super::settings::TaskTemplateItem {
            task: "retrieval_query".to_string(),
            template: "自定义 {0}".to_string(),
        }];
        apply_profile_defaults("ollama/qwen3-embedding:0.6b", &mut caps, &mut templates);
        assert_eq!(templates[0].template, "自定义 {0}");
    }

    #[test]
    fn apply_profile_unknown_model_noop() {
        let mut caps = vec![EmbeddingCapability::OutputDimensions];
        let mut templates = Vec::new();
        apply_profile_defaults("ollama/unknown", &mut caps, &mut templates);
        assert_eq!(caps, vec![EmbeddingCapability::OutputDimensions]);
        assert!(templates.is_empty());
    }
}
