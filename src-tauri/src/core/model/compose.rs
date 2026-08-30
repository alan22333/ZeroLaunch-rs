//! embedding 请求文本组装：能力校验与语义任务模板渲染。
//!
//! 组装责任在 provider（契约：不同模型输入格式不同，由 provider 内部组装）；
//! 本模块提供两个 provider 共用的能力校验与模板渲染。

use super::model_profiles::{gemma_fallback_template, template_for, SemanticTask};
use zerolaunch_plugin_api::services::model::{EmbeddingCapability, ModelError};

/// 解析语义任务类型字符串；未知任务返回 None。
fn parse_semantic_task(task_type: &str) -> Option<SemanticTask> {
    SemanticTask::ALL
        .iter()
        .find(|t| t.as_str() == task_type)
        .copied()
}

/// 渲染模板：`{0}` 替换为 text，`{1}` 起按顺序替换为 args。
///
/// 校验：模板引用的占位符索引不得超过可用参数（`{0}` 恒可用 = text）；
/// 越界（模板 `{2}` 但 args 仅 1 个）返回 InvalidRequest。
fn render_template(template: &str, text: &str, args: &[String]) -> Result<String, ModelError> {
    let mut out = String::with_capacity(template.len() + text.len());
    let mut rest = template;
    while let Some(pos) = rest.find('{') {
        // 占位符前的普通文本原样保留。
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        // 找闭合 }，解析数字索引。
        let Some(close) = after.find('}') else {
            out.push('{');
            out.push_str(after);
            return Ok(out);
        };
        let idx_str = &after[..close];
        if let Ok(idx) = idx_str.parse::<usize>() {
            let value = if idx == 0 {
                Some(text)
            } else {
                args.get(idx - 1).map(String::as_str)
            };
            match value {
                Some(v) => {
                    out.push_str(v);
                    rest = &after[close + 1..];
                }
                None => {
                    return Err(ModelError::InvalidRequest(format!(
                        "模板占位符 {{{idx}}} 超出可用参数（template_args 长度 {}）",
                        args.len()
                    )));
                }
            }
        } else {
            // 非数字占位符：原样保留。
            out.push('{');
            out.push_str(&after[..close]);
            out.push('}');
            rest = &after[close + 1..];
        }
    }
    out.push_str(rest);
    Ok(out)
}

/// 校验 embedding 请求参数与模型能力声明一致。
///
/// 输入：批量长度、可选任务类型、模型能力；不修改输入。
/// 错误：任务类型未声明能力、任务类型未知时返回 InvalidRequest。
pub(crate) fn validate_embedding_request(
    task_type: Option<&str>,
    capabilities: &[EmbeddingCapability],
) -> Result<(), ModelError> {
    if task_type.is_some() && !capabilities.contains(&EmbeddingCapability::TaskType) {
        return Err(ModelError::InvalidRequest(
            "模型未声明支持 taskType 能力".to_string(),
        ));
    }
    if let Some(task) = task_type {
        if parse_semantic_task(task).is_none() {
            return Err(ModelError::InvalidRequest(format!(
                "未知的 task_type: {task}"
            )));
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

/// 组装 embedding 最终输入文本：能力声明校验 + 模板渲染。
///
/// 输入：原始文本列表、可选任务类型、模型能力、用户任务模板映射、模型 id（查内置档案）、
/// 与 input 一一对应的模板参数（每个 input 一个，`{1}` 起顺序填充）。
/// 返回：按模型模板渲染后的文本列表（长度与 input 一致）。
///
/// 模板优先级：用户配置 → 内置模型档案 → gemma 回退。
/// 无模板（档案与用户配置均缺失且 task_type 为空）时原样透传 input。
pub(crate) fn compose_embedding_texts(
    input: &[String],
    task_type: Option<&str>,
    capabilities: &[EmbeddingCapability],
    user_templates: &[super::settings::TaskTemplateItem],
    model_id: &str,
    template_args: Option<&[Vec<String>]>,
) -> Result<Vec<String>, ModelError> {
    validate_embedding_request(task_type, capabilities)?;
    let args_for = |idx: usize| -> &[String] {
        template_args
            .and_then(|args| args.get(idx))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    };

    match task_type {
        None => Ok(input.to_vec()),
        Some(task) => {
            // 模板：用户配置 → 内置档案 → gemma 回退。
            let task = parse_semantic_task(task).expect("已校验");
            let template = template_for(task, user_templates, model_id)
                .or_else(|| gemma_fallback_template(task).map(String::from))
                .expect("模板查询恒命中（回退保底）");
            input
                .iter()
                .enumerate()
                .map(|(idx, text)| render_template(&template, text, args_for(idx)))
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(items: &[EmbeddingCapability]) -> Vec<EmbeddingCapability> {
        items.to_vec()
    }

    fn empty_templates() -> Vec<super::super::settings::TaskTemplateItem> {
        Vec::new()
    }

    #[test]
    fn plain_text_task_passes_through() {
        // plain_text 任务：qwen3 档案模板为 {0}，渲染后原文直传。
        let out = compose_embedding_texts(
            &["聊天".to_string()],
            Some("plain_text"),
            &caps(&[EmbeddingCapability::TaskType]),
            &empty_templates(),
            "ollama/qwen3-embedding:0.6b",
            None,
        )
        .unwrap();
        assert_eq!(out, vec!["聊天".to_string()]);
    }

    #[test]
    fn plain_text_task_gemma_fallback_passes_through() {
        // 无档案模型的 plain_text 任务走 gemma 回退 {0}，同样原文直传。
        let out = compose_embedding_texts(
            &["聊天".to_string()],
            Some("plain_text"),
            &caps(&[EmbeddingCapability::TaskType]),
            &empty_templates(),
            "ollama/unknown-model",
            None,
        )
        .unwrap();
        assert_eq!(out, vec!["聊天".to_string()]);
    }

    #[test]
    fn qwen3_retrieval_query_uses_instruct_template() {
        let out = compose_embedding_texts(
            &["聊天".to_string()],
            Some("retrieval_query"),
            &caps(&[EmbeddingCapability::TaskType]),
            &empty_templates(),
            "ollama/qwen3-embedding:0.6b",
            None,
        )
        .unwrap();
        assert_eq!(
            out,
            vec!["Instruct: Given a web search query, retrieve relevant passages that answer the query\nQuery:聊天".to_string()]
        );
    }

    #[test]
    fn qwen3_retrieval_document_passes_through() {
        let out = compose_embedding_texts(
            &["body".to_string()],
            Some("retrieval_document"),
            &caps(&[EmbeddingCapability::TaskType]),
            &empty_templates(),
            "ollama/qwen3-embedding:0.6b",
            None,
        )
        .unwrap();
        assert_eq!(out, vec!["body".to_string()]);
    }

    #[test]
    fn gemma_document_template_uses_official_title_none() {
        // 官方模板文档侧 title 恒为字面量 none，不依赖 template_args。
        let out = compose_embedding_texts(
            &["body".to_string()],
            Some("retrieval_document"),
            &caps(&[EmbeddingCapability::TaskType]),
            &empty_templates(),
            "ollama/gemma-embedding:300m",
            None,
        )
        .unwrap();
        assert_eq!(out, vec!["title: none | text: body".to_string()]);
    }

    #[test]
    fn user_template_overrides_builtin() {
        let user = vec![super::super::settings::TaskTemplateItem {
            task: "retrieval_query".to_string(),
            template: "自定义 {0}".to_string(),
        }];
        let out = compose_embedding_texts(
            &["hello".to_string()],
            Some("retrieval_query"),
            &caps(&[EmbeddingCapability::TaskType]),
            &user,
            "ollama/qwen3-embedding:0.6b",
            None,
        )
        .unwrap();
        assert_eq!(out, vec!["自定义 hello".to_string()]);
    }

    #[test]
    fn template_placeholder_out_of_range_rejected() {
        // 模板引用 {1} 但无 template_args → 报错。
        let user = vec![super::super::settings::TaskTemplateItem {
            task: "retrieval_query".to_string(),
            template: "前缀 {1} 后缀".to_string(),
        }];
        let err = compose_embedding_texts(
            &["hello".to_string()],
            Some("retrieval_query"),
            &caps(&[EmbeddingCapability::TaskType]),
            &user,
            "ollama/qwen3-embedding:0.6b",
            None,
        )
        .unwrap_err();
        assert!(matches!(err, ModelError::InvalidRequest(_)));
    }

    #[test]
    fn multi_placeholder_template_renders_in_order() {
        let user = vec![super::super::settings::TaskTemplateItem {
            task: "retrieval_document".to_string(),
            template: "{2} | {1} | {0}".to_string(),
        }];
        let args = vec![vec!["A".to_string(), "B".to_string()]];
        let out = compose_embedding_texts(
            &["text".to_string()],
            Some("retrieval_document"),
            &caps(&[EmbeddingCapability::TaskType]),
            &user,
            "ollama/qwen3-embedding:0.6b",
            Some(&args),
        )
        .unwrap();
        assert_eq!(out, vec!["B | A | text".to_string()]);
    }

    #[test]
    fn unknown_task_type_rejected() {
        let err = compose_embedding_texts(
            &["x".to_string()],
            Some("bogus"),
            &caps(&[EmbeddingCapability::TaskType]),
            &empty_templates(),
            "ollama/m",
            None,
        )
        .unwrap_err();
        assert!(matches!(err, ModelError::InvalidRequest(_)));
    }
}
