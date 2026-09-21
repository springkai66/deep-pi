//! 桌宠浮窗：常驻桌面的独立小窗口（透明、置顶、不进任务栏）。
//!
//! 与看板浮窗同构：无边框窗口加载 `/pet` 预渲染页。差异点：
//! - 位置由用户自由拖放，持久化在 `pet/pet-state.json`（物理像素），独立于
//!   settings.json —— 主窗口保存设置是整份覆写，混进设置会互相覆盖坐标。
//! - 形象三种来源：内置机器人（默认）、本地图片（复制进 pet 资产目录）、
//!   AI 生成（用当前配置的模型生成 SVG，经 `call_configured_model`）。
//!   形象变更后向桌宠窗口广播 `pet-appearance` 事件即时换装。

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

use crate::message::{msg, msg_with};
use crate::settings::SettingsStore;

/// 桌宠窗口标签。能力清单（capabilities/pet-window.json）按它授权。
pub const PET_LABEL: &str = "pet";
/// 桌宠窗口尺寸（物理像素近似值；逻辑像素按 DPI 换算的差异可接受）。
pub const PET_WIDTH: u32 = 210;
pub const PET_HEIGHT: u32 = 240;
/// 默认停靠边距：右缘留 24px，底部留 88px（常见任务栏高度 + 余量）。
const DEFAULT_MARGIN_RIGHT: i32 = 24;
const DEFAULT_MARGIN_BOTTOM: i32 = 88;
/// 自定义形象文件大小上限（复制进资产目录前检查）。
const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;
/// AI 生成的 SVG 大小上限。
const MAX_GENERATED_SVG_BYTES: usize = 512 * 1024;
/// 形象描述的长度上限（与提示词增强同量级）。
const MAX_PROMPT_CHARS: usize = 500;
/// 允许的自定义形象扩展名 → MIME。
const IMAGE_MIME: [(&str, &str); 5] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("webp", "image/webp"),
    ("svg", "image/svg+xml"),
];

/// open_pet_window 的在途创建标志：并发触发时只允许一次 build。
static PET_OPEN_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// 桌宠持久化状态。image = None 表示内置机器人；Some(文件名) 指向
/// pet 目录里的形象文件（只存文件名，杜绝路径穿越）。
#[derive(Serialize, Deserialize, Default, PartialEq, Eq, Clone, Debug)]
pub(crate) struct PetState {
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
    #[serde(default)]
    image: Option<String>,
}

/// `pet/` 目录读写：pet-state.json（原子写）+ 形象资产文件。
pub(crate) struct PetStateStore {
    dir: PathBuf,
}

impl PetStateStore {
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn state_path(&self) -> PathBuf {
        self.dir.join("pet-state.json")
    }

    fn ensure_dir(&self) -> Result<(), String> {
        fs::create_dir_all(&self.dir).map_err(|error| format!("failed to create pet dir: {error}"))
    }

    /// 读取状态；文件缺失或损坏返回默认值（下次保存时覆写）。
    pub(crate) fn load(&self) -> PetState {
        fs::read_to_string(self.state_path())
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub(crate) fn save(&self, state: PetState) -> Result<(), String> {
        self.ensure_dir()?;
        let mut file = atomic_write_file::AtomicWriteFile::open(self.state_path())
            .map_err(|error| format!("failed to open pet state: {error}"))?;
        serde_json::to_writer(&mut file, &state)
            .map_err(|error| format!("failed to serialize pet state: {error}"))?;
        file.commit()
            .map_err(|error| format!("failed to save pet state: {error}"))
    }

    /// 形象文件路径；name 只允许纯文件名，杜绝路径穿越。
    fn asset_path(&self, name: &str) -> Option<PathBuf> {
        if name.is_empty() || name.contains(['/', '\\', ':']) || name.starts_with('.') {
            return None;
        }
        Some(self.dir.join(name))
    }

    /// 删除一个本目录内的形象资产文件（存在才删）。
    fn remove_asset(&self, name: &str) {
        if let Some(path) = self.asset_path(name) {
            let _ = fs::remove_file(path);
        }
    }
}

fn pet_state(app: &AppHandle) -> Option<PetStateStore> {
    let paths = app.try_state::<crate::app_paths::AppPaths>()?;
    Some(PetStateStore::new(paths.pet.clone()))
}

/// 当前生效的形象：内置机器人，或某个形象文件的字节（base64 + MIME）。
#[derive(Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PetAppearance {
    Builtin,
    Image { mime: String, data_base64: String },
}

/// 打开（或显示已存在的）桌宠浮窗。
#[tauri::command]
pub async fn open_pet_window(app: AppHandle) -> Result<(), String> {
    open_or_focus(&app)
}

/// 启动时按设置自动显示桌宠。设置未就绪或已关闭时不显示。
pub fn show_pet_at_startup(app: &AppHandle) {
    let enabled = app
        .try_state::<SettingsStore>()
        .and_then(|store| store.get().ok())
        .map(|settings| settings.pet_enabled)
        .unwrap_or(false);
    if !enabled {
        return;
    }
    if let Err(error) = open_or_focus(app) {
        log::warn!("event=pet_startup_open error={error}");
    }
}

fn open_or_focus(app: &AppHandle) -> Result<(), String> {
    if let Some(pet) = app.get_webview_window(PET_LABEL) {
        let _ = pet.unminimize();
        let _ = pet.show();
        return Ok(());
    }
    // 并发触发时只允许一次 build；撞上时静默返回，窗口总会建成。
    if PET_OPEN_IN_FLIGHT.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let result = build_pet_window(app);
    PET_OPEN_IN_FLIGHT.store(false, Ordering::SeqCst);
    result
}

fn build_pet_window(app: &AppHandle) -> Result<(), String> {
    // 必须用无扩展名路由路径（同 board）：预渲染页 + 资产解析器回退。
    let pet = WebviewWindowBuilder::new(app, PET_LABEL, WebviewUrl::App("pet".into()))
        .title("DeepPi")
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .closable(true)
        .shadow(false)
        .focused(false)
        .inner_size(PET_WIDTH as f64, PET_HEIGHT as f64)
        .visible(false)
        .build()
        .map_err(|error| format!("failed to create pet window: {error}"))?;
    position_pet(app, &pet);
    // 不抢焦点：桌宠只是陪伴，不应打断正在输入的窗口。
    let _ = pet.show();
    Ok(())
}

/// 计算并应用桌宠位置：优先恢复保存值（钳位），否则停靠主显示器右下角。
fn position_pet(app: &AppHandle, pet: &tauri::WebviewWindow) {
    let monitors = collect_monitors(app);
    let position = pet_state(app).map(|store| store.load());
    let (x, y) = match position {
        Some(state) => clamp_pet_position(state.x, state.y, PET_WIDTH, PET_HEIGHT, &monitors),
        None => default_pet_position(PET_WIDTH, PET_HEIGHT, &monitors),
    };
    let _ = pet.set_position(PhysicalPosition::new(x, y));
}

/// 收集所有显示器的物理坐标区域 (x, y, width, height)。
fn collect_monitors(app: &AppHandle) -> Vec<(i32, i32, u32, u32)> {
    app.available_monitors()
        .ok()
        .map(|monitors| {
            monitors
                .iter()
                .map(|monitor| {
                    (
                        monitor.position().x,
                        monitor.position().y,
                        monitor.size().width,
                        monitor.size().height,
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 把桌宠位置钳进包含它的显示器；中心点不在任何显示器内时取「桌宠中心
/// 距显示器中心最近」的一块。坐标为物理像素，允许负值（多显示器向左/向
/// 上扩展）。显示器比桌宠还小时贴显示器原点。
fn clamp_pet_position(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    monitors: &[(i32, i32, u32, u32)],
) -> (i32, i32) {
    if monitors.is_empty() {
        return (x, y);
    }
    let width = width.max(1) as i32;
    let height = height.max(1) as i32;
    let center = (x + width / 2, y + height / 2);
    let monitor = monitors
        .iter()
        .copied()
        .find(|(mx, my, mw, mh)| {
            center.0 >= *mx
                && center.0 < mx + *mw as i32
                && center.1 >= *my
                && center.1 < my + *mh as i32
        })
        .unwrap_or_else(|| {
            *monitors
                .iter()
                .min_by_key(|(mx, my, mw, mh)| {
                    let dx = center.0 - (mx + (*mw as i32) / 2);
                    let dy = center.1 - (my + (*mh as i32) / 2);
                    dx * dx + dy * dy
                })
                .expect("monitors is non-empty")
        });
    let (mx, my, mw, mh) = monitor;
    let max_x = (mx + mw as i32 - width).max(mx);
    let max_y = (my + mh as i32 - height).max(my);
    (x.clamp(mx, max_x), y.clamp(my, max_y))
}

/// 默认停靠位置：主显示器（原点所在块，否则第一块）右下角，留出任务栏。
fn default_pet_position(width: u32, height: u32, monitors: &[(i32, i32, u32, u32)]) -> (i32, i32) {
    let Some((mx, my, mw, mh)) = monitors
        .iter()
        .copied()
        .find(|(mx, my, _, _)| *mx == 0 && *my == 0)
        .or_else(|| monitors.first().copied())
    else {
        return (0, 0);
    };
    let x = mx + mw as i32 - width as i32 - DEFAULT_MARGIN_RIGHT;
    let y = my + mh as i32 - height as i32 - DEFAULT_MARGIN_BOTTOM;
    clamp_pet_position(x, y, width, height, monitors)
}

/// 保存桌宠位置（物理像素）。桌宠窗口在拖动结束后调用（前端已防抖）。
#[tauri::command]
pub async fn save_pet_position(app: AppHandle, x: i32, y: i32) -> Result<(), String> {
    let store = pet_state(&app).ok_or_else(|| "app state not ready".to_string())?;
    let mut state = store.load();
    state.x = x;
    state.y = y;
    store.save(state)
}

/// 读取当前桌宠形象（内置或形象文件字节）。形象文件丢失时自愈回落内置。
#[tauri::command]
pub async fn get_pet_appearance(app: AppHandle) -> Result<PetAppearance, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let store = pet_state(&app).ok_or_else(|| "app state not ready".to_string())?;
        let state = store.load();
        let Some(name) = state.image.clone() else {
            return Ok(PetAppearance::Builtin);
        };
        let Some(path) = store.asset_path(&name) else {
            return Ok(PetAppearance::Builtin);
        };
        let Ok(bytes) = fs::read(&path) else {
            // 形象文件丢失（被手动清理）：回落内置并清掉悬空引用。
            let mut healed = state;
            healed.image = None;
            let _ = store.save(healed);
            return Ok(PetAppearance::Builtin);
        };
        let Some(mime) = mime_for(&name) else {
            return Ok(PetAppearance::Builtin);
        };
        Ok(PetAppearance::Image {
            mime: mime.to_owned(),
            data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 设置自定义形象：校验扩展名与大小，复制进 pet 资产目录后启用。
#[tauri::command]
pub async fn set_pet_image(app: AppHandle, path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let source = PathBuf::from(&path);
        let extension = source
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| msg("pet.image.unsupported"))?;
        if !IMAGE_MIME.iter().any(|(name, _)| *name == extension) {
            return Err(msg("pet.image.unsupported"));
        }
        let metadata = fs::metadata(&source).map_err(|_| msg("pet.image.missing"))?;
        if metadata.len() > MAX_IMAGE_BYTES {
            return Err(msg_with("pet.image.too_large", &[("max", "5")]));
        }
        let bytes = fs::read(&source).map_err(|_| msg("pet.image.missing"))?;
        // 内容嗅探：扩展名只是提示，实际格式以文件头为准，防伪装文件。
        let Some(sniffed) = sniff_image_extension(&bytes) else {
            return Err(msg("pet.image.unsupported"));
        };
        let store = pet_state(&app).ok_or_else(|| "app state not ready".to_string())?;
        store.ensure_dir()?;
        let name = format!("custom-{}.{}", unix_millis(), sniffed);
        fs::write(
            store.asset_path(&name).expect("safe generated name"),
            &bytes,
        )
        .map_err(|error| format!("failed to save pet image: {error}"))?;
        apply_appearance(&app, &store, Some(name))?;
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 恢复内置机器人形象。
#[tauri::command]
pub async fn reset_pet_image(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let store = pet_state(&app).ok_or_else(|| "app state not ready".to_string())?;
        apply_appearance(&app, &store, None)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 用当前配置的模型生成桌宠形象：让模型输出一个 Q 版 SVG（文本模型即可
/// 生成，不要求图像模型），校验后存入 pet 资产目录并启用。桌宠窗口用
/// `<img src="data:image/svg+xml">` 渲染 —— img 内嵌 SVG 天然不执行脚本。
#[tauri::command]
pub async fn generate_pet_image(app: AppHandle, prompt: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || generate_pet_image_sync(&app, &prompt))
        .await
        .map_err(|error| error.to_string())?
}

fn generate_pet_image_sync(app: &AppHandle, prompt: &str) -> Result<(), String> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err(msg("pet.generate.input_empty"));
    }
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        return Err(msg_with(
            "pet.generate.input_too_long",
            &[("max", &MAX_PROMPT_CHARS.to_string())],
        ));
    }
    let paths = app
        .try_state::<crate::app_paths::AppPaths>()
        .map(|paths| paths.inner().clone())
        .ok_or_else(|| "app state not ready".to_string())?;
    let text = crate::provider::call_configured_model(
        &paths,
        PET_SVG_SYSTEM_PROMPT,
        prompt,
        Duration::from_secs(120),
        4096,
    )
    .map_err(generate_error)?;
    let svg = extract_svg(&text).ok_or_else(|| msg("pet.generate.no_svg"))?;
    let store = pet_state(app).ok_or_else(|| "app state not ready".to_string())?;
    store.ensure_dir()?;
    let name = format!("ai-{}.svg", unix_millis());
    fs::write(store.asset_path(&name).expect("safe generated name"), svg)
        .map_err(|error| format!("failed to save generated pet: {error}"))?;
    apply_appearance(app, &store, Some(name))
}

/// 桌宠形象生成的系统提示词：约束为单根 SVG、透明背景、禁止脚本。
const PET_SVG_SYSTEM_PROMPT: &str = "你是桌宠形象设计师。用户会描述一个桌宠形象，请把它画成一个可爱的 Q 版 SVG。严格要求：只输出一个 <svg> 根元素，不要输出任何解释、注释或代码块围栏；viewBox 必须是 \"0 0 200 240\"；背景保持透明；用简洁的色块与圆润线条，形象居中、占满画布；不要包含 <script>、<foreignObject>、<image>、外部引用或任何脚本与事件属性。";

/// 危险内容检测：脚本/foreignObject/内嵌位图/use 引用/任何 href（含
/// javascript: 与外链）/事件属性（on*）。桌宠窗口虽然以 `<img>` 的安全
/// 静态模式渲染 SVG（不执行脚本、不加载外部资源），这里仍做深度防御。
fn svg_looks_dangerous(svg: &str) -> bool {
    let lower = svg.to_ascii_lowercase();
    if lower.contains("<script")
        || lower.contains("<foreignobject")
        || lower.contains("<image")
        || lower.contains("<use")
        || lower.contains("javascript:")
        || lower.contains("data:text/html")
    {
        return true;
    }
    // 事件属性：o n 后面跟字母再跟 =（如 onload=、onerror=）。
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b' ' && bytes[i + 1] == b'o' {
            let mut j = i + 2;
            while j < bytes.len() && bytes[j].is_ascii_alphabetic() {
                j += 1;
            }
            if j > i + 2 && j < bytes.len() && bytes[j] == b'=' {
                return true;
            }
        }
        i += 1;
    }
    // 任何 href/xlink:href（含外链与内页跳转）一律拒绝。
    if lower.contains("href") {
        return true;
    }
    // CSS 里的外链资源引用（url(#id) 本地引用除外）。
    if lower.contains("url(") && !lower.contains("url(#") {
        return true;
    }
    false
}

/// 从模型输出里提取 SVG：剥掉围栏，取第一个 <svg 到最后一个 </svg>；
/// 拒绝含危险内容与超限体积。
fn extract_svg(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let start = trimmed.find("<svg")?;
    let end = trimmed.rfind("</svg>")? + "</svg>".len();
    if end <= start {
        return None;
    }
    let svg = &trimmed[start..end];
    if svg.len() > MAX_GENERATED_SVG_BYTES {
        return None;
    }
    if svg_looks_dangerous(svg) {
        return None;
    }
    Some(svg.to_owned())
}

/// 共用的模型调用错误 → 消息码映射（与提示词增强一致，Local 已是消息码）。
fn generate_error(error: crate::provider::ChatCallError) -> String {
    match error {
        crate::provider::ChatCallError::ProviderUnconfigured => {
            msg("prompt_enhance.provider_unconfigured")
        }
        crate::provider::ChatCallError::BaseUrlMissing => msg("prompt_enhance.base_url_missing"),
        crate::provider::ChatCallError::Request(error) => {
            msg_with("prompt_enhance.request_failed", &[("error", &error)])
        }
        crate::provider::ChatCallError::Http(status) => msg_with(
            "prompt_enhance.http_failed",
            &[("status", &status.to_string())],
        ),
        crate::provider::ChatCallError::ResponseInvalid(error) => {
            msg_with("pet.generate.response_invalid", &[("error", &error)])
        }
        crate::provider::ChatCallError::Local(error) => error,
    }
}

/// 启用新形象并广播给桌宠窗口；旧形象文件是本目录生成的资产时一并清理。
fn apply_appearance(
    app: &AppHandle,
    store: &PetStateStore,
    image: Option<String>,
) -> Result<(), String> {
    let mut state = store.load();
    if let Some(previous) = state.image.take() {
        // 只清理本目录管理的形象文件（state 里的名字已过 asset_path 校验）。
        store.remove_asset(&previous);
    }
    state.image = image;
    store.save(state)?;
    // 形象变更广播：发给桌宠窗口（未打开则无收件人，静默即可）。
    let _ = app.emit_to(PET_LABEL, "pet-appearance", ());
    Ok(())
}

/// 按文件头嗅探图片格式；svg 额外要求文本内含 <svg 根元素。
/// 返回与 IMAGE_MIME 一致的扩展名（用于命名资产文件）。
fn sniff_image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("png");
    }
    if bytes.starts_with(b"\xff\xd8\xff") {
        return Some("jpg");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    if bytes.starts_with(b"GIF8") {
        return Some("gif");
    }
    let start = bytes.iter().position(|b| !b.is_ascii_whitespace())?;
    let trimmed = &bytes[start..];
    if trimmed.starts_with(b"<svg")
        || (trimmed.starts_with(b"<?xml") && trimmed.windows(4).any(|w| w == b"<svg"))
    {
        return Some("svg");
    }
    None
}

fn mime_for(name: &str) -> Option<&'static str> {
    let extension = name.rsplit('.').next()?;
    IMAGE_MIME
        .iter()
        .find(|(candidate, _)| *candidate == extension)
        .map(|(_, mime)| *mime)
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_position_inside_containing_monitor() {
        // 主显示器 1920×1080：桌宠贴右下角时不出屏。
        let monitors = [(0, 0, 1920, 1080)];
        assert_eq!(
            clamp_pet_position(5000, 5000, 210, 240, &monitors),
            (1920 - 210, 1080 - 240)
        );
        assert_eq!(clamp_pet_position(-50, -50, 210, 240, &monitors), (0, 0));
        assert_eq!(
            clamp_pet_position(100, 100, 210, 240, &monitors),
            (100, 100)
        );
    }

    #[test]
    fn snaps_to_nearest_monitor_when_center_is_off_screen() {
        // 左侧负坐标副屏：中心落在其内 → 钳进副屏。
        let monitors = [(-1920, 0, 1920, 1080), (0, 0, 1920, 1080)];
        assert_eq!(
            clamp_pet_position(-2000, 100, 210, 240, &monitors),
            (-1920, 100)
        );
        // 中心在所有屏幕之外最远处 → 最近的主屏。
        assert_eq!(
            clamp_pet_position(9000, 100, 210, 240, &monitors),
            (1920 - 210, 100)
        );
    }

    #[test]
    fn pins_to_monitor_origin_when_monitor_is_smaller_than_pet() {
        let monitors = [(0, 0, 100, 80)];
        assert_eq!(clamp_pet_position(10, 10, 210, 240, &monitors), (0, 0));
    }

    #[test]
    fn passes_through_without_monitors() {
        assert_eq!(clamp_pet_position(-7, 9, 210, 240, &[]), (-7, 9));
    }

    #[test]
    fn defaults_to_bottom_right_of_primary_monitor() {
        let monitors = [(-1920, 0, 1920, 1080), (0, 0, 1920, 1080)];
        let (x, y) = default_pet_position(210, 240, &monitors);
        assert_eq!(x, 1920 - 210 - 24);
        assert_eq!(y, 1080 - 240 - 88);
    }

    #[test]
    fn pet_state_store_round_trips_and_tolerates_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let store = PetStateStore::new(dir.path().join("pet"));
        assert_eq!(store.load(), PetState::default());
        store
            .save(PetState {
                x: -192,
                y: 88,
                image: Some("ai-1.svg".into()),
            })
            .unwrap();
        assert_eq!(
            store.load(),
            PetState {
                x: -192,
                y: 88,
                image: Some("ai-1.svg".into()),
            }
        );
        // 损坏文件：读取回落默认值而不是报错。
        fs::write(dir.path().join("pet").join("pet-state.json"), "{oops").unwrap();
        assert_eq!(store.load(), PetState::default());
    }

    #[test]
    fn asset_path_rejects_path_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let store = PetStateStore::new(dir.path().to_path_buf());
        assert!(store.asset_path("ai-1.svg").is_some());
        assert!(store.asset_path("../settings.json").is_none());
        assert!(store.asset_path("a\\b.svg").is_none());
        assert!(store.asset_path("C:\\evil.svg").is_none());
        assert!(store.asset_path(".hidden").is_none());
        assert!(store.asset_path("").is_none());
    }

    #[test]
    fn extracts_svg_and_strips_surroundings() {
        let fenced =
            "好的，这是桌宠：\n```svg\n<svg viewBox=\"0 0 200 240\"><circle cx=\"1\"/></svg>\n```";
        assert_eq!(
            extract_svg(fenced).unwrap(),
            "<svg viewBox=\"0 0 200 240\"><circle cx=\"1\"/></svg>"
        );
        // 无 SVG / 只有开始标签 / 超限体积 → None。
        assert!(extract_svg("没有图形").is_none());
        assert!(extract_svg("<svg>未闭合").is_none());
        let huge = format!("<svg>{}</svg>", "x".repeat(MAX_GENERATED_SVG_BYTES + 1));
        assert!(extract_svg(&huge).is_none());
    }

    #[test]
    fn sniffs_image_formats_by_magic_bytes() {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(b"rest");
        assert_eq!(sniff_image_extension(&png), Some("png"));
        assert_eq!(sniff_image_extension(b"\xff\xd8\xff\xe0jpeg"), Some("jpg"));
        assert_eq!(
            sniff_image_extension(b"RIFF\x00\x00\x00\x00WEBPVP8 "),
            Some("webp")
        );
        assert_eq!(sniff_image_extension(b"GIF89a"), Some("gif"));
        assert_eq!(
            sniff_image_extension(b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>"),
            Some("svg")
        );
        assert_eq!(
            sniff_image_extension(b"<?xml version=\"1.0\"?><svg/>"),
            Some("svg")
        );
        assert_eq!(sniff_image_extension(b"MZ fake exe"), None);
        assert_eq!(sniff_image_extension(b""), None);
    }

    #[test]
    fn rejects_dangerous_svg_content() {
        assert!(extract_svg("<svg><script>alert(1)</script></svg>").is_none());
        assert!(extract_svg("<svg><foreignObject/></svg>").is_none());
        assert!(extract_svg("<svg><image href='http://x'/></svg>").is_none());
        assert!(extract_svg("<svg><a xlink:href='http://x'/></svg>").is_none());
        assert!(extract_svg("<svg onload='x()'/> ").is_none());
        assert!(extract_svg("<svg><rect onmouseover='x()'/></svg>").is_none());
        assert!(extract_svg("<svg><a href='http://x'/></svg>").is_none());
        assert!(extract_svg("<svg><use href='#x'/></svg>").is_none());
        assert!(extract_svg("<svg><style>@import url(http://x)</style></svg>").is_none());
        // url(#) 本地引用允许。
        assert!(extract_svg("<svg><style>fill:url(#a)</style></svg>").is_some());
    }
}
