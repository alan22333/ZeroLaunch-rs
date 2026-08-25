use crate::host::HostApiError;

/// 宿主实际生效的界面主题。
///
/// 由宿主在配置模式与系统主题之间完成解析；插件只消费最终的浅色或深色结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    /// 浅色主题；宿主当前界面使用浅色配色。
    Light,
    /// 深色主题；宿主当前界面使用深色配色。
    Dark,
}

/// 提供宿主当前实际生效主题的平台能力。
///
/// 由平台层实现并注入 HostApi，PluginHandle 仅负责向插件转发查询结果。
pub trait ThemeProvider: Send + Sync {
    /// 查询当前系统主题；宿主显式配置的 light/dark 由 HostApi 在上层优先处理。
    fn current_system_theme(&self) -> Result<Theme, HostApiError>;
}
