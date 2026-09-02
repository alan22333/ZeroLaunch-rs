//! Compile-time platform selection for HostApi dependency injection.

#[cfg(target_os = "windows")]
pub use zerolaunch_platform_windows::{
    start_system_theme_monitor, windows_capabilities as platform_capabilities,
    WindowsAppEnumerator as PlatformAppEnumerator, WindowsAppLauncher as PlatformAppLauncher,
    WindowsAutoStartManager as PlatformAutoStartManager,
    WindowsClipboardManager as PlatformClipboardManager,
    WindowsClipboardProvider as PlatformClipboardProvider,
    WindowsFocusMonitor as PlatformFocusMonitor, WindowsHotkeyManager as PlatformHotkeyManager,
    WindowsIconExtractor as PlatformIconExtractor,
    WindowsInstallationMonitor as PlatformInstallationMonitor,
    WindowsLnkResolver as PlatformLnkResolver, WindowsPathResolver as PlatformPathResolver,
    WindowsResourceLoader as PlatformResourceLoader,
    WindowsSelectionProvider as PlatformSelectionProvider,
    WindowsShellExecutor as PlatformShellExecutor, WindowsThemeProvider as PlatformThemeProvider,
    WindowsWindowHandleProvider as PlatformWindowHandleProvider,
    WindowsWindowManager as PlatformWindowManager,
    WindowsWindowPositioner as PlatformWindowPositioner,
};

#[cfg(target_os = "macos")]
pub use zerolaunch_platform_macos::{
    macos_capabilities as platform_capabilities, start_system_theme_monitor,
    MacosAppEnumerator as PlatformAppEnumerator, MacosAppLauncher as PlatformAppLauncher,
    MacosAutoStartManager as PlatformAutoStartManager,
    MacosClipboardManager as PlatformClipboardManager,
    MacosClipboardProvider as PlatformClipboardProvider, MacosFocusMonitor as PlatformFocusMonitor,
    MacosHotkeyManager as PlatformHotkeyManager, MacosIconExtractor as PlatformIconExtractor,
    MacosInstallationMonitor as PlatformInstallationMonitor,
    MacosLnkResolver as PlatformLnkResolver, MacosPathResolver as PlatformPathResolver,
    MacosResourceLoader as PlatformResourceLoader,
    MacosSelectionProvider as PlatformSelectionProvider,
    MacosShellExecutor as PlatformShellExecutor, MacosThemeProvider as PlatformThemeProvider,
    MacosWindowHandleProvider as PlatformWindowHandleProvider,
    MacosWindowManager as PlatformWindowManager, MacosWindowPositioner as PlatformWindowPositioner,
};
