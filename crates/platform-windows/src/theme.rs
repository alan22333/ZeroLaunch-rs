use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;
use zerolaunch_plugin_api::host::HostApiError;
use zerolaunch_plugin_api::services::{Theme, ThemeProvider};

/// Windows 系统主题提供器。
///
/// 读取 Windows 个性化注册表中的应用主题设置；仅由 platform-windows 注入 HostApi 使用。
pub struct WindowsThemeProvider;

impl Default for WindowsThemeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsThemeProvider {
    /// 创建 Windows 系统主题提供器。
    pub fn new() -> Self {
        Self
    }
}

impl ThemeProvider for WindowsThemeProvider {
    /// 读取 AppsUseLightTheme，将 0 转换为深色，其余有效值转换为浅色。
    fn current_system_theme(&self) -> Result<Theme, HostApiError> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu
            .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
            .map_err(|error| HostApiError::ExecutionFailed {
                service: "theme".to_string(),
                reason: error.to_string(),
            })?;
        let value: u32 =
            key.get_value("AppsUseLightTheme")
                .map_err(|error| HostApiError::ExecutionFailed {
                    service: "theme".to_string(),
                    reason: error.to_string(),
                })?;
        Ok(if value == 0 {
            Theme::Dark
        } else {
            Theme::Light
        })
    }
}
