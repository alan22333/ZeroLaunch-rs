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
        if plugin_id.is_empty() || !is_valid_plugin_id(&plugin_id) {
            return Err("invalid plugin id".into());
        }

        if !path.starts_with("ui/") {
            return Err("access denied: only ui/ path allowed".into());
        }

        let plugin_dir = self.plugins_dir.join(&plugin_id);
        let asset_path = plugin_dir.join(&path);
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
fn parse_resource(uri: &str) -> Option<(String, String)> {
    let resource_path = uri
        .strip_prefix("http://zlplugin.localhost/")
        .or_else(|| uri.strip_prefix("https://zlplugin.localhost/"))
        .or_else(|| uri.strip_prefix("zlplugin://"))?;
    let resource_path = resource_path
        .strip_prefix("localhost/")
        .unwrap_or(resource_path);
    // 剥离查询串（如 ?v=0.2.0 面板版本失效参数），路径校验基于干净路径
    let resource_path = resource_path.split('?').next().unwrap_or(resource_path);
    // 百分号解码：含空格/中文的资源路径（如 ui/我的面板.mjs）此前 404；
    // 解码后 %2e%2e 会还原为 ../，由调用方 ui/ 前缀 + canonicalize 双重拦截
    let (plugin_id, path) = resource_path.split_once('/')?;
    let plugin_id = percent_decode_str(plugin_id)?;
    let path = percent_decode_str(path)?;
    Some((plugin_id, path))
}

/// 百分号解码 URI 路径段；非法编码（如孤立 %）返回 None。
fn percent_decode_str(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            let byte = u8::from_str_radix(hex, 16).ok()?;
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
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
            Some((
                "com.ghost-him.everything".to_string(),
                "ui/panel.mjs".to_string()
            ))
        );
        assert_eq!(
            parse_resource("http://zlplugin.localhost/com.example.hello/ui/panel.mjs?v=0.2.0"),
            Some(("com.example.hello".to_string(), "ui/panel.mjs".to_string()))
        );
        assert_eq!(
            parse_resource("https://zlplugin.localhost/com.example.hello/ui/panel.mjs?v=0.2.0&x=1"),
            Some(("com.example.hello".to_string(), "ui/panel.mjs".to_string()))
        );
    }

    #[test]
    fn percent_decode_handles_encoded_chars() {
        assert_eq!(
            percent_decode_str("ui/my%20panel.mjs").as_deref(),
            Some("ui/my panel.mjs")
        );
        assert_eq!(
            percent_decode_str("ui/%E6%88%91%E7%9A%84%E9%9D%A2%E6%9D%BF.mjs").as_deref(),
            Some("ui/我的面板.mjs")
        );
        assert_eq!(
            percent_decode_str("plain.mjs").as_deref(),
            Some("plain.mjs")
        );
    }

    #[test]
    fn percent_decode_rejects_invalid() {
        assert_eq!(percent_decode_str("ui/%zz.mjs"), None); // 非法 hex 对
                                                            // 结尾孤立 %（后无 hex 对）：原样保留，与浏览器 URI 解码一致，非错误
        assert_eq!(percent_decode_str("ui/%2").as_deref(), Some("ui/%2"));
        assert_eq!(percent_decode_str("ui/%").as_deref(), Some("ui/%"));
        // 截断的多字节：%E6 %88 各自是合法 hex 对，但拼出的字节不是合法 UTF-8 → None
        assert_eq!(percent_decode_str("ui/%E6%88"), None);
    }

    #[test]
    fn percent_decode_dotdot_stays_detectable() {
        // %2e%2e 解码后还原为 ..（供上层 ui/ 前缀 + canonicalize 拦截）
        assert_eq!(
            percent_decode_str("%2e%2e/secret").as_deref(),
            Some("../secret")
        );
    }

    #[test]
    fn parse_resource_decodes_path() {
        assert_eq!(
            parse_resource("zlplugin://com.example.hello/ui/my%20panel.mjs"),
            Some((
                "com.example.hello".to_string(),
                "ui/my panel.mjs".to_string()
            ))
        );
    }

    #[test]
    fn parse_resource_rejects_invalid() {
        assert_eq!(parse_resource("http://example.com/x"), None);
        assert_eq!(parse_resource("zlplugin://localhost/no-plugin-id"), None);
        assert_eq!(parse_resource("not-a-uri"), None);
    }
}
