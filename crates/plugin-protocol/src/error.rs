use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("toml: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("semver: {0}")]
    Semver(#[from] semver::Error),

    #[error("invalid frame: {0}")]
    InvalidFrame(String),

    #[error("rpc error: code={code} message={message}")]
    Rpc { code: i32, message: String },

    #[error("timeout")]
    Timeout,

    #[error("transport closed")]
    TransportClosed,

    #[error("protocol version incompatible: plugin {plugin} declares {got}, host expects {expected} (major version must match)")]
    ProtocolVersionIncompatible {
        plugin: String,
        expected: String,
        got: String,
    },

    #[error("manifest error: {0}")]
    Manifest(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// JSON-RPC 2.0 标准与自定义错误码。
pub mod codes {
    /// 解析错误：消息体不是合法 JSON。
    pub const PARSE_ERROR: i32 = -32700;
    /// 无效请求：JSON 合法但不是合法的请求对象。
    pub const INVALID_REQUEST: i32 = -32600;
    /// 方法不存在。
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// 无效参数：参数缺失或类型不匹配。
    pub const INVALID_PARAMS: i32 = -32602;
    /// 内部错误：处理请求时发生未归类异常。
    pub const INTERNAL_ERROR: i32 = -32603;
    /// 插件执行错误：插件处理请求返回错误。
    pub const PLUGIN_ERROR: i32 = -32000;
    /// 插件进程崩溃/异常退出。
    pub const PLUGIN_CRASHED: i32 = -32001;
    /// 请求超时：插件在限时内未响应。
    pub const TIMEOUT_ERROR: i32 = -32002;
    /// 不支持的组件类型：插件声明了宿主未注册的组件。
    pub const UNSUPPORTED_COMPONENT: i32 = -32003;
    /// 模型不存在（model_id 未注册）。
    pub const MODEL_NOT_FOUND: i32 = -32100;
    /// 模型提供方不可达/未就绪（Ollama 未启动、网络失败等）。
    pub const MODEL_PROVIDER_UNAVAILABLE: i32 = -32101;
    /// 模型请求载荷非法。
    pub const MODEL_INVALID_REQUEST: i32 = -32102;
    /// 模型提供方不支持该操作。
    pub const MODEL_NOT_SUPPORTED: i32 = -32103;
    /// 模型传输失败。
    pub const MODEL_TRANSPORT: i32 = -32104;
}
