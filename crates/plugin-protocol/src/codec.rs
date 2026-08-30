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
/// - base64 字母表（A-Za-z0-9+/=）字符占比超过 98%——
///   普通文本/JSON 含空格、标点与非 ASCII 字符，占比会显著更低。
fn looks_like_base64(s: &str) -> bool {
    if s.len() <= 64 {
        return false;
    }
    // base64 字母表（A-Za-z0-9+/=）覆盖绝大多数字符即判定为疑似二进制。
    // 普通文本含空格、标点与非 ASCII 字符，占比会被显著拉低。
    let total = s.chars().count() as f64;
    let base64_chars = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
        .count() as f64;
    base64_chars / total > 0.98
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

    /// 纯小写字母长串（无空格）是合法 base64 形态，应被摘要。
    #[test]
    fn summarize_masks_pure_lowercase_long_string() {
        let s = "a".repeat(80);
        let summarized = summarize_value(&json!(s));
        let out = summarized.as_str().expect("仍是字符串");
        assert!(
            out.starts_with("<base64 len="),
            "纯小写长串应被摘要: {}",
            out
        );
    }

    /// 带空格的英文长文本不算 base64，原样保留。
    #[test]
    fn summarize_keeps_long_english_with_spaces() {
        let text = "the quick brown fox jumps over the lazy dog ".repeat(10);
        assert_eq!(summarize_value(&json!(text)), json!(text));
    }
}
