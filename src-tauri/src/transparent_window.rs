//! Keep the main window's transparent canvas free of Windows backdrop materials.
use tauri::WebviewWindow;
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMSBT_NONE, DWMWA_SYSTEMBACKDROP_TYPE,
};

/// Configure once at startup; theme opacity is controlled entirely by CSS.
pub fn configure(window: &WebviewWindow) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &DWMSBT_NONE as *const _ as _,
            std::mem::size_of_val(&DWMSBT_NONE) as u32,
        )
    }
    .map_err(|error| error.to_string())
}
