//! embedding 请求文本组装：按模型声明的能力拼接标题/任务前缀。
//!
//! 组装责任在 provider（契约：不同模型输入格式不同，由 provider 内部组装）；
//! 本模块提供两个 provider 共用的 gemma 模板与能力校验。

use zerolaunch_plugin_api::services::model::{EmbeddingCapability, ModelError};

/// gemma 任务类型 → 输入前缀模板。
///
/// Google EmbeddingGemma 官方模板（ai.google.dev/gemma/docs/embeddinggemma）：
/// 查询侧 `task: {task} | query: {text}`，文档侧 `title: {title} | text: {text}`
/// （无标题时 title 为 `none`）。
fn task_prefix(task_type: &str) -> Result<&'static str, ModelError> {
    match task_type {
        // 文档侧无 task 前缀，由 title 模板处理。
        "retrieval_document" => Ok(""),
        "retrieval_query" => Ok("task: search result | query: "),
        "semantic_similarity" => Ok("task: sentence similarity | query: "),
        "classification" => Ok("task: classification | query: "),
        "clustering" => Ok("task: clustering | query: "),
        other => Err(ModelError::InvalidRequest(format!(
            "未知的 task_type: {other}"
        ))),
    }
}

/// 校验 embedding 请求参数与模型能力声明一致。
///
/// 输入：批量长度、可选标题、可选任务类型、模型能力；不修改输入。
/// 错误：能力未声明、标题数量不匹配、任务类型未知或查询任务携带标题时返回 InvalidRequest。
pub(crate) fn validate_embedding_request(
    input_len: usize,
    titles: Option<&[String]>,
    task_type: Option<&str>,
    capabilities: &[EmbeddingCapability],
) -> Result<(), ModelError> {
    if titles.is_some() && !capabilities.contains(&EmbeddingCapability::Title) {
        return Err(ModelError::InvalidRequest(
            "模型未声明支持 title 能力".to_string(),
        ));
    }
    if task_type.is_some() && !capabilities.contains(&EmbeddingCapability::TaskType) {
        return Err(ModelError::InvalidRequest(
            "模型未声明支持 taskType 能力".to_string(),
        ));
    }
    if let Some(ts) = titles {
        if ts.len() != input_len {
            return Err(ModelError::InvalidRequest(
                "titles 数量与 input 不一致".to_string(),
            ));
        }
    }
    if let Some(task) = task_type {
        if !task_prefix(task)?.is_empty() && titles.is_some() {
            return Err(ModelError::InvalidRequest(
                "查询类任务不支持标题输入".to_string(),
            ));
        }
    }
    Ok(())
}

/// 校验单项 embedding 能力声明。
///
/// 输入：模型能力清单、所需能力及其显示名称；未声明时返回 InvalidRequest。
pub(crate) fn require_embedding_capability(
    capabilities: &[EmbeddingCapability],
    capability: EmbeddingCapability,
    label: &str,
) -> Result<(), ModelError> {
    if capabilities.contains(&capability) {
        return Ok(());
    }
    Err(ModelError::InvalidRequest(format!(
        "模型未声明支持 {label} 能力"
    )))
}

/// 组装 embedding 最终输入文本：能力声明校验 + 标题/任务前缀拼接。
///
/// 输入：原始文本列表、可选标题（与 input 一一对应）、可选任务类型、模型声明能力。
/// 返回：按模型模板组装后的文本列表（长度与 input 一致）。
pub(crate) fn compose_embedding_texts(
    input: &[String],
    titles: Option<&[String]>,
    task_type: Option<&str>,
    capabilities: &[EmbeddingCapability],
) -> Result<Vec<String>, ModelError> {
    validate_embedding_request(input.len(), titles, task_type, capabilities)?;

    match task_type {
        None => match titles {
            Some(ts) => Ok(input
                .iter()
                .zip(ts.iter())
                .map(|(text, title)| format!("title: {title} | text: {text}"))
                .collect()),
            None => Ok(input.to_vec()),
        },
        Some(task) => {
            let prefix = task_prefix(task)?;
            if prefix.is_empty() {
                // 文档侧：有标题用标题模板，无标题补 title: none。
                match titles {
                    Some(ts) => Ok(input
                        .iter()
                        .zip(ts.iter())
                        .map(|(text, title)| format!("title: {title} | text: {text}"))
                        .collect()),
                    None => Ok(input
                        .iter()
                        .map(|t| format!("title: none | text: {t}"))
                        .collect()),
                }
            } else {
                Ok(input.iter().map(|t| format!("{prefix}{t}")).collect())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(items: &[EmbeddingCapability]) -> Vec<EmbeddingCapability> {
        items.to_vec()
    }

    #[test]
    fn no_capability_request_passes_through() {
        let out = compose_embedding_texts(&["hello".to_string()], None, None, &caps(&[])).unwrap();
        assert_eq!(out, vec!["hello".to_string()]);
    }

    #[test]
    fn titles_rejected_without_title_capability() {
        let err = compose_embedding_texts(
            &["hello".to_string()],
            Some(&["t".to_string()]),
            None,
            &caps(&[]),
        )
        .unwrap_err();
        assert!(matches!(err, ModelError::InvalidRequest(_)));
    }

    #[test]
    fn title_capability_composes_gemma_template() {
        let out = compose_embedding_texts(
            &["content".to_string()],
            Some(&["My Title".to_string()]),
            None,
            &caps(&[EmbeddingCapability::Title]),
        )
        .unwrap();
        assert_eq!(out, vec!["title: My Title | text: content".to_string()]);
    }

    #[test]
    fn task_type_composes_query_prefix() {
        let out = compose_embedding_texts(
            &["how to win".to_string()],
            None,
            Some("retrieval_query"),
            &caps(&[EmbeddingCapability::TaskType]),
        )
        .unwrap();
        assert_eq!(
            out,
            vec!["task: search result | query: how to win".to_string()]
        );
    }

    #[test]
    fn retrieval_document_without_title_uses_none() {
        let out = compose_embedding_texts(
            &["body".to_string()],
            None,
            Some("retrieval_document"),
            &caps(&[EmbeddingCapability::TaskType]),
        )
        .unwrap();
        assert_eq!(out, vec!["title: none | text: body".to_string()]);
    }

    #[test]
    fn retrieval_document_with_title_uses_title_template() {
        let out = compose_embedding_texts(
            &["body".to_string()],
            Some(&["Doc".to_string()]),
            Some("retrieval_document"),
            &caps(&[EmbeddingCapability::TaskType, EmbeddingCapability::Title]),
        )
        .unwrap();
        assert_eq!(out, vec!["title: Doc | text: body".to_string()]);
    }

    #[test]
    fn query_task_rejects_titles() {
        let err = compose_embedding_texts(
            &["q".to_string()],
            Some(&["t".to_string()]),
            Some("retrieval_query"),
            &caps(&[EmbeddingCapability::TaskType, EmbeddingCapability::Title]),
        )
        .unwrap_err();
        assert!(matches!(err, ModelError::InvalidRequest(_)));
    }

    #[test]
    fn unknown_task_type_rejected() {
        let err = compose_embedding_texts(
            &["x".to_string()],
            None,
            Some("bogus"),
            &caps(&[EmbeddingCapability::TaskType]),
        )
        .unwrap_err();
        assert!(matches!(err, ModelError::InvalidRequest(_)));
    }

    #[test]
    fn titles_length_mismatch_rejected() {
        let err = compose_embedding_texts(
            &["a".to_string(), "b".to_string()],
            Some(&["t".to_string()]),
            None,
            &caps(&[EmbeddingCapability::Title]),
        )
        .unwrap_err();
        assert!(matches!(err, ModelError::InvalidRequest(_)));
    }
}
