//! zlplugin:// 协议处理器。
//!
//! 从 PluginManager 中提取的自定义 URI 协议处理职责域，
//! 处理 `http://zlplugin.localhost/<plugin-id>/ui/<sub-path>` 格式的请求。

use std::path::{Path, PathBuf};

use regex::Regex;

/// zlplugin:// 协议处理器。
pub(crate) struct ZlpluginProtocolHandler {
    plugins_dir: PathBuf,
}

impl ZlpluginProtocolHandler {
    /// 创建处理器，指定插件根目录。
    pub(crate) fn new(plugins_dir: PathBuf) -> Self {
        Self { plugins_dir }
    }

    /// 处理 `zlplugin://` 协议请求，返回 (文件字节, MIME 类型)。
    ///
    /// URI 格式：`zlplugin://localhost/<plugin-id>/ui/<sub-path>`。
    pub(crate) fn handle(
        &self,
        uri: &str,
    ) -> Result<(Vec<u8>, String), Box<dyn std::error::Error>> {
        let (plugin_id, path) = parse_resource(uri).ok_or("not a zlplugin URI")?;
        if plugin_id.is_empty() || !is_valid_plugin_id(plugin_id) {
            return Err("invalid plugin id".into());
        }

        if !path.starts_with("ui/") {
            return Err("access denied: only ui/ path allowed".into());
        }

        let plugin_dir = self.plugins_dir.join(plugin_id);
        let asset_path = plugin_dir.join(path);
        // Canonicalize 防路径遍历
        let canonical = asset_path.canonicalize()?;
        let plugin_canonical = plugin_dir.canonicalize()?;
        if !canonical.starts_with(&plugin_canonical) {
            return Err("access denied: path traversal detected".into());
        }

        let bytes = std::fs::read(&canonical)?;
        let mime = mime_from_extension(&canonical).to_string();

        Ok((bytes, mime))
    }
}

// ── 私有辅助函数 ─────────────────────────────────────────────────

/// 解析 zlplugin URI → (插件 id, 资源路径)；剥离查询参数。非法 URI 返回 None。
fn parse_resource(uri: &str) -> Option<(&str, &str)> {
    let resource_path = uri
        .strip_prefix("http://zlplugin.localhost/")
        .or_else(|| uri.strip_prefix("https://zlplugin.localhost/"))
        .or_else(|| uri.strip_prefix("zlplugin://"))?;
    let resource_path = resource_path
        .strip_prefix("localhost/")
        .unwrap_or(resource_path);
    // 剥离查询串（如 ?v=0.2.0 面板版本失效参数），路径校验基于干净路径
    let resource_path = resource_path.split('?').next().unwrap_or(resource_path);
    resource_path.split_once('/')
}

/// 校验插件 ID 是否符合反向域名格式。
fn is_valid_plugin_id(id: &str) -> bool {
    use std::sync::LazyLock;
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(zerolaunch_plugin_protocol::manifest::PLUGIN_ID_RE).unwrap());
    RE.is_match(id)
}

/// 根据文件扩展名确定 MIME 类型。
fn mime_from_extension(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("mjs") | Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("html") => "text/html",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("woff") | Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

// ── 单元测试 ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_resource_supports_scheme_prefixes() {
        assert_eq!(
            parse_resource("zlplugin://com.ghost-him.everything/ui/panel.mjs"),
            Some(("com.ghost-him.everything", "ui/panel.mjs"))
        );
        assert_eq!(
            parse_resource("http://zlplugin.localhost/com.example.hello/ui/panel.mjs?v=0.2.0"),
            Some(("com.example.hello", "ui/panel.mjs"))
        );
        assert_eq!(
            parse_resource("https://zlplugin.localhost/com.example.hello/ui/panel.mjs?v=0.2.0&x=1"),
            Some(("com.example.hello", "ui/panel.mjs"))
        );
    }

    #[test]
    fn parse_resource_rejects_invalid() {
        assert_eq!(parse_resource("http://example.com/x"), None);
        assert_eq!(parse_resource("zlplugin://localhost/no-plugin-id"), None);
        assert_eq!(parse_resource("not-a-uri"), None);
    }
}
