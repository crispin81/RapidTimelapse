// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Some wlroots-based Wayland compositors (seen on Hyprland) hit a fatal
/// `Error 71 (Protocol error) dispatching to Wayland display` in GTK4's
/// native Wayland backend, and separately a `Failed to create GBM buffer`
/// from WebKitGTK's DMA-BUF renderer that leaves the window blank even when
/// it doesn't crash outright. Both are worked around by falling back to
/// XWayland and software compositing. Set *before* Tauri/GTK initializes;
/// harmless on platforms where GTK/WebKitGTK aren't in play (macOS,
/// Windows), and never overrides a value the user already set themselves.
#[cfg(target_os = "linux")]
fn apply_linux_webkit_workarounds() {
    for (key, value) in [
        // Prefer X11/XWayland, but still fall back to native Wayland on
        // Wayland-only systems where this bug doesn't occur.
        ("GDK_BACKEND", "x11,wayland"),
        ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
        ("WEBKIT_DISABLE_COMPOSITING_MODE", "1"),
    ] {
        if std::env::var(key).is_err() {
            std::env::set_var(key, value);
        }
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    apply_linux_webkit_workarounds();

    rrtimelapse_lib::run()
}
