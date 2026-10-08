//! Applies a theme-selected native backdrop material to the main window only.
use tauri::WebviewWindow;

#[cfg(windows)]
fn set_dwm_acrylic(window: &WebviewWindow, enabled: bool) -> Result<(), String> {
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMSBT_NONE, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
    };

    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    let backdrop = if enabled {
        DWMSBT_TRANSIENTWINDOW
    } else {
        DWMSBT_NONE
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

#[derive(serde::Serialize)]
pub struct NativeMaterialStatus {
    material: &'static str,
    fallback: Option<&'static str>,
}

fn select_native_material(
    material: &str,
    transparency_enabled: bool,
) -> Result<NativeMaterialStatus, String> {
    let requested_acrylic = material_is_acrylic(material)?;
    Ok(if requested_acrylic && !transparency_enabled {
        NativeMaterialStatus {
            material: "transparent",
            fallback: Some("system_transparency_disabled"),
        }
    } else {
        NativeMaterialStatus {
            material: if requested_acrylic { "acrylic" } else { "none" },
            fallback: None,
        }
    })
}

#[cfg(windows)]
fn system_transparency_enabled() -> bool {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|key| key.get_value::<u32, _>("EnableTransparency"))
        .map(|value| value != 0)
        .unwrap_or(true)
}

/// Apply or clear a theme material after verifying the invoking webview is the main window.
#[tauri::command]
pub fn set_main_window_material(
    window: WebviewWindow,
    material: String,
) -> Result<NativeMaterialStatus, String> {
    if window.label() != "main" {
        return Err("native material is restricted to the main window".into());
    }
    #[cfg(windows)]
    let status = select_native_material(&material, system_transparency_enabled())?;
    #[cfg(not(windows))]
    let status = select_native_material(&material, true)?;
    let enabled = status.material == "acrylic";
    #[cfg(windows)]
    {
        let result = match set_dwm_acrylic(&window, enabled) {
            Ok(()) => {
                log::info!(
                    "event=window_material status=applied backend=dwm material={} fallback={:?}",
                    status.material,
                    status.fallback
                );
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

    Ok(status)
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

    #[test]
    fn preserves_transparency_when_system_acrylic_is_disabled() {
        let result = super::select_native_material("acrylic", false).unwrap();
        assert_eq!(result.material, "transparent");
        assert_eq!(result.fallback, Some("system_transparency_disabled"));
        let enabled = super::select_native_material("acrylic", true).unwrap();
        assert_eq!(enabled.material, "acrylic");
        assert_eq!(enabled.fallback, None);
        let solid = super::select_native_material("none", false).unwrap();
        assert_eq!(solid.material, "none");
        assert_eq!(solid.fallback, None);
        assert!(super::select_native_material("mica", false).is_err());
    }
}
