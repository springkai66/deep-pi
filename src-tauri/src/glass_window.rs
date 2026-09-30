//! Applies a theme-selected native backdrop material to the main window only.
use tauri::WebviewWindow;

#[cfg(windows)]
fn set_dwm_acrylic(window: &WebviewWindow, enabled: bool) -> Result<(), String> {
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMSBT_AUTO, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
    };

    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    let backdrop = if enabled {
        DWMSBT_TRANSIENTWINDOW
    } else {
        DWMSBT_AUTO
    };
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const _ as _,
            std::mem::size_of_val(&backdrop) as u32,
        )
    }
    .map_err(|error| error.to_string())
}

fn material_is_acrylic(material: &str) -> Result<bool, String> {
    match material {
        "none" => Ok(false),
        "acrylic" => Ok(true),
        _ => Err("material must be none or acrylic".into()),
    }
}

/// Apply or clear a theme material after verifying the invoking webview is the main window.
#[tauri::command]
pub fn set_main_window_material(window: WebviewWindow, material: String) -> Result<(), String> {
    if window.label() != "main" {
        return Err("native material is restricted to the main window".into());
    }
    let enabled = material_is_acrylic(&material)?;
    #[cfg(windows)]
    {
        let result = match set_dwm_acrylic(&window, enabled) {
            Ok(()) => {
                log::info!("event=window_material status=applied backend=dwm material={material}");
                Ok(())
            }
            Err(dwm_error) => {
                let fallback = if enabled {
                    window_vibrancy::apply_acrylic(&window, None)
                } else {
                    window_vibrancy::clear_acrylic(&window)
                };
                match fallback {
                    Ok(()) => {
                        log::info!("event=window_material status=applied backend=accent_fallback material={material}");
                        Ok(())
                    }
                    Err(fallback_error) => Err(format!(
                        "DWM: {dwm_error}; Acrylic fallback: {fallback_error}"
                    )),
                }
            }
        };
        if let Err(error) = result {
            log::warn!(
                "event=window_material status=failed material={material} reason=\"{error}\""
            );
            return Err(error);
        }
    }

    #[cfg(not(windows))]
    let _ = (window, enabled);

    Ok(())
}
#[cfg(test)]
mod tests {
    use super::material_is_acrylic;

    #[test]
    fn only_accepts_supported_native_materials() {
        assert_eq!(material_is_acrylic("none"), Ok(false));
        assert_eq!(material_is_acrylic("acrylic"), Ok(true));
        assert!(material_is_acrylic("mica").is_err());
    }
}
