//! LSP Content-Length 帧的编码工具。
//!
//! 本模块提供纯同步函数，不依赖 tokio。异步 I/O 由各 crate 自行处理。

use crate::jsonrpc::{Message, Notification, Request, Response};

/// 单帧的最大字节数（16 MB）。
/// 超过此大小的帧视为无效，防止内存溢出。
pub const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

/// LSP 帧头部的最大字节数限制（512 字节）。
/// 防止恶意或损坏的发送方发送无限长的头部行。
pub const MAX_HEADER_SIZE: usize = 512;

/// 将 payload 编码为完整的 LSP Content-Length 帧格式。
///
/// 返回 `Content-Length: N\r\n\r\n{payload}` 格式的字节序列，
/// 可直接写入任意 `AsyncWrite` 或 `Write`。
///
pub fn encode_frame(payload: &[u8]) -> Vec<u8> {
    let header = format!("Content-Length: {}\r\n\r\n", payload.len());
    let mut frame = Vec::with_capacity(header.len() + payload.len());
    frame.extend_from_slice(header.as_bytes());
    frame.extend_from_slice(payload);
    frame
}

/// 判断字符串是否为“疑似二进制数据”（如 base64 编码的图标/资源字节）。
///
/// 启发式规则：
/// - 长度超过阈值（64 字符）；
/// - 包含至少一个数字；
/// - 字符集高度偏向 base64 字母表（A-Za-z0-9+/=），
///   且大量大写字母与/或 +/ 字符——普通文本/JSON 很少出现这种形态。
fn looks_like_base64(s: &str) -> bool {
    if s.len() <= 64 {
        return false;
    }
    let mut alnum = 0usize;
    let mut digits = 0usize;
    let mut upper = 0usize;
    let mut symbols = 0usize;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            alnum += 1;
        }
        if c.is_ascii_digit() {
            digits += 1;
        }
        if c.is_ascii_uppercase() {
            upper += 1;
        }
        if matches!(c, '+' | '/' | '=') {
            symbols += 1;
        }
    }
    let total = s.chars().count() as f64;
    (digits > 0 && (alnum as f64 / total) > 0.95 && (upper as f64 / total) > 0.25)
        || ((symbols as f64 / total) > 0.02 && (alnum as f64 / total) > 0.9)
}

/// 将 JSON 值转换为适合 debug 日志展示的形式：
/// 文本结构完整保留，疑似二进制的长 base64 字符串替换为
/// `"<base64 len=... prefix=...>"` 摘要，避免日志被二进制数据刷屏。
pub fn summarize_value(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::String(s) if looks_like_base64(s) => {
            let prefix: String = s.chars().take(64).collect();
            serde_json::Value::String(format!("<base64 len={} prefix={}>", s.len(), prefix))
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(summarize_value).collect())
        }
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, val)| (k.clone(), summarize_value(val)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// 将 JSON-RPC 消息转换为适合 debug 日志完整展示的格式
/// （二进制数据字段被摘要，其余原样）。
pub fn summarize_message(msg: &Message) -> Message {
    match msg {
        Message::Request(req) => Message::Request(Request {
            jsonrpc: req.jsonrpc.clone(),
            id: req.id,
            method: req.method.clone(),
            params: summarize_value(&req.params),
        }),
        Message::Response(resp) => Message::Response(Response {
            jsonrpc: resp.jsonrpc.clone(),
            id: resp.id,
            result: resp.result.as_ref().map(summarize_value),
            error: resp.error.clone(),
        }),
        Message::Notification(notif) => Message::Notification(Notification {
            jsonrpc: notif.jsonrpc.clone(),
            method: notif.method.clone(),
            params: summarize_value(&notif.params),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 普通文本结构必须完整保留（含嵌套数组/对象）。
    #[test]
    fn summarize_keeps_text_structure() {
        let v = json!({
            "name": "hello",
            "list": [1, 2, {"deep": "world"}],
        });
        assert_eq!(summarize_value(&v), v);
    }

    /// 短 base64 字符串（< 64 字符）不算二进制，原样展示。
    #[test]
    fn summarize_keeps_short_base64() {
        let s = "aGVsbG8=";
        assert_eq!(summarize_value(&json!(s)), json!("aGVsbG8="));
    }

    /// 长 base64 字符串（如图标/资源字节）替换为摘要。
    #[test]
    fn summarize_masks_long_base64() {
        // 手工构造的 base64 样例："ZeroLaunch" 重复 20 次的 base64 编码
        // （WmVyb0xhdW5jaA== 是 "ZeroLaunch" 的 base64）。
        let unit = "WmVyb0xhdW5jaA==";
        let b64 = unit.repeat(20);
        let summarized = summarize_value(&json!(b64));
        let s = summarized.as_str().expect("仍是字符串");
        assert!(s.starts_with("<base64 len="), "应被摘要: {}", s);
        assert!(s.contains(&b64[..16]), "摘要应包含前缀");
    }

    /// 普通长文本（如模型回复）即使很长也不应被误判为二进制。
    #[test]
    fn summarize_keeps_long_text() {
        let text = "这是普通的模型回复文本，".repeat(50);
        assert_eq!(summarize_value(&json!(text)), json!(text));
    }

    /// 嵌套结构中的长 base64 也应被摘要（对象/数组递归）。
    #[test]
    fn summarize_masks_nested_base64() {
        let v = json!({
            "meta": { "ok": true },
            "data": [{"bytes": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}]
        });
        let out = summarize_value(&v);
        assert!(out["meta"]["ok"] == json!(true), "非二进制字段保持完整");
        let masked = out["data"][0]["bytes"].as_str().unwrap();
        assert!(
            masked.starts_with("<base64 len="),
            "嵌套 base64 应被摘要: {}",
            masked
        );
    }
}
