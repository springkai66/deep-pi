//! 桌宠浮窗：常驻桌面的独立小窗口（透明、置顶、不进任务栏）。
//!
//! 与看板浮窗同构：无边框窗口加载 `/pet` 预渲染页。差异点：
//! - 位置由用户自由拖放，持久化在 `pet/pet-state.json`（物理像素），独立于
//!   settings.json —— 主窗口保存设置是整份覆写，混进设置会互相覆盖坐标。
//! - 形象两种来源：默认形象（随应用打包的内置图片 pet-default.jpg）、
//!   本地图片（复制进 pet 资产目录）。形象变更后向桌宠窗口广播
//!   `pet-appearance` 事件即时换装。

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

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
/// 允许的自定义形象扩展名 → MIME（与 sniff_image_extension 的返回一致）。
const IMAGE_MIME: [(&str, &str); 6] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("webp", "image/webp"),
    ("gif", "image/gif"),
    ("svg", "image/svg+xml"),
];
/// 默认桌宠形象：随应用打包的内置图片（未选择自定义形象时生效）。
const DEFAULT_PET_IMAGE: &[u8] = include_bytes!("../assets/pet-default.jpg");

/// open_pet_window 的在途创建标志：并发触发时只允许一次 build。
static PET_OPEN_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
/// 桌宠窗口的 DPI 缩放（×1000 缓存）：壁纸钩子坐标是物理像素，窗口尺寸/
/// 锚点是逻辑像素，换算需要它。建窗时刷新；未建窗时按 1.0 处理。
static PET_SCALE_MILLIS: AtomicU32 = AtomicU32::new(1000);

/// 桌宠持久化状态。image = None 表示内置默认图片；Some(文件名) 指向
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

/// 当前生效的形象：内置默认图，或某个自定义形象文件的字节（base64 + MIME）。
#[derive(Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PetAppearance {
    Image {
        mime: String,
        data_base64: String,
        /// true 表示尚未选择自定义形象（返回的是内置默认图）。
        is_default: bool,
    },
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
    // 壁纸点击监听（爬行/闪现）随桌宠启用；幂等，重复调用无副作用。
    crate::desktop_hook::install(app.clone());
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

/// 当前缓存缩放（物理像素 ↔ 逻辑像素换算用）。
fn cached_pet_scale() -> f64 {
    PET_SCALE_MILLIS.load(Ordering::SeqCst) as f64 / 1000.0
}

/// 刷新缩放缓存（建窗时与桌面钩子手势起点调用）。
pub(crate) fn store_pet_scale(scale: f64) {
    PET_SCALE_MILLIS.store((scale * 1000.0).round() as u32, Ordering::SeqCst);
}

/// 当前缩放下的窗口物理尺寸（逻辑尺寸 × 缩放）。
fn physical_pet_size(scale: f64) -> (u32, u32) {
    (
        (PET_WIDTH as f64 * scale).round() as u32,
        (PET_HEIGHT as f64 * scale).round() as u32,
    )
}

fn build_pet_window(app: &AppHandle) -> Result<(), String> {
    // 必须用无扩展名路由路径（同 board）：预渲染页 + 资产解析器回退。
    let pet = WebviewWindowBuilder::new(app, PET_LABEL, WebviewUrl::App("pet".into()))
        .title("DeepPi")
        .decorations(false)
        .transparent(true)
        .always_on_top(pet_always_on_top(app))
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
    // 先缓存缩放再定位：位置钳位需要物理尺寸。
    store_pet_scale(pet.scale_factor().unwrap_or(1.0));
    position_pet(app, &pet);
    // 不抢焦点：桌宠只是陪伴，不应打断正在输入的窗口。
    let _ = pet.show();
    Ok(())
}

/// 桌宠是否置顶（设置驱动；读取失败回落置顶，保持既有行为）。
fn pet_always_on_top(app: &AppHandle) -> bool {
    app.try_state::<SettingsStore>()
        .and_then(|store| store.get().ok())
        .map(|settings| settings.pet_always_on_top)
        .unwrap_or(true)
}

/// 应用桌宠置顶设置到已打开的浮窗（未打开则无操作）。设置保存时调用，
/// 让「桌宠置顶显示」开关即时生效。
pub(crate) fn apply_always_on_top(app: &AppHandle, on_top: bool) {
    if let Some(pet) = app.get_webview_window(PET_LABEL) {
        let _ = pet.set_always_on_top(on_top);
    }
}

/// 计算并应用桌宠位置：优先恢复保存值（钳位），否则停靠主显示器右下角。
fn position_pet(app: &AppHandle, pet: &tauri::WebviewWindow) {
    let monitors = collect_monitors(app);
    let (width, height) = physical_pet_size(cached_pet_scale());
    let position = pet_state(app).map(|store| store.load());
    let (x, y) = match position {
        Some(state) => clamp_pet_position(state.x, state.y, width, height, &monitors),
        None => default_pet_position(width, height, &monitors),
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

/// 桌宠本体（图片区域）中心在窗口内的锚点（逻辑像素）：窗口 210×240，
/// 本体 150×150 贴底居中、底部留白 10px → 中心 ≈ (105, 155)。壁纸点击
/// 坐标（物理）减去锚点×缩放即窗口左上角目标位置。
const ANCHOR_X: f64 = 105.0;
const ANCHOR_Y: f64 = 155.0;
/// 任务环绕气泡模式下的窗口尺寸（逻辑像素，与前端 RING 常量一致）。
pub(crate) const RING_WIDTH: f64 = 400.0;
pub(crate) const RING_HEIGHT: f64 = 320.0;

/// 壁纸点击坐标（物理）→ 桌宠窗口左上角目标位置（物理，锚定本体中心
/// 并钳进屏幕）。
pub(crate) fn anchored_pet_position(app: &AppHandle, cursor_x: i32, cursor_y: i32) -> (i32, i32) {
    let scale = cached_pet_scale();
    let monitors = collect_monitors(app);
    clamp_pet_position(
        cursor_x - (ANCHOR_X * scale).round() as i32,
        cursor_y - (ANCHOR_Y * scale).round() as i32,
        physical_pet_size(scale).0,
        physical_pet_size(scale).1,
        &monitors,
    )
}

/// 展开/收起任务环绕气泡：改窗口尺寸（逻辑）并按缩放补偿物理位置，
/// 保持桌宠本体在屏幕上不动。
#[tauri::command]
pub async fn set_pet_ring(app: AppHandle, open: bool) -> Result<(), String> {
    let Some(pet) = app.get_webview_window(PET_LABEL) else {
        return Ok(());
    };
    tauri::async_runtime::spawn_blocking(move || {
        let scale = cached_pet_scale();
        let (width, height) = if open {
            (RING_WIDTH, RING_HEIGHT)
        } else {
            (PET_WIDTH as f64, PET_HEIGHT as f64)
        };
        let (delta_x, delta_y) = ring_resize_delta(open);
        let current = pet.outer_position().map_err(|error| error.to_string())?;
        let monitors = collect_monitors(&app);
        let (x, y) = clamp_pet_position(
            current.x + (delta_x * scale).round() as i32,
            current.y + (delta_y * scale).round() as i32,
            (width * scale).round() as u32,
            (height * scale).round() as u32,
            &monitors,
        );
        pet.set_size(LogicalSize::new(width, height))
            .map_err(|error| error.to_string())?;
        pet.set_position(PhysicalPosition::new(x, y))
            .map_err(|error| error.to_string())?;
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 展开时窗口左上角应移动的逻辑位移（收起取反）：宽 (210−400)/2 = −95，
/// 高 240−320 = −80。展开时窗口向左上扩，本体才能停在原位。
fn ring_resize_delta(open: bool) -> (f64, f64) {
    let dx = (PET_WIDTH as f64 - RING_WIDTH) / 2.0;
    let dy = PET_HEIGHT as f64 - RING_HEIGHT;
    if open {
        (dx, dy)
    } else {
        (-dx, -dy)
    }
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

/// 读取当前桌宠形象（自定义形象文件字节，否则内置默认图）。自定义形象
/// 文件丢失或格式无法识别时自愈回落默认图。
#[tauri::command]
pub async fn get_pet_appearance(app: AppHandle) -> Result<PetAppearance, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let store = pet_state(&app).ok_or_else(|| "app state not ready".to_string())?;
        let state = store.load();
        if let Some(name) = state.image.clone() {
            if let Some(path) = store.asset_path(&name) {
                if let Ok(bytes) = fs::read(&path) {
                    if let Some(mime) = mime_for(&name) {
                        return Ok(PetAppearance::Image {
                            mime: mime.to_owned(),
                            data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
                            is_default: false,
                        });
                    }
                }
            }
            // 形象文件丢失/不可识别（被手动清理）：回落默认并清掉悬空引用。
            let mut healed = state;
            healed.image = None;
            let _ = store.save(healed);
        }
        Ok(PetAppearance::Image {
            mime: "image/jpeg".to_owned(),
            data_base64: base64::engine::general_purpose::STANDARD.encode(DEFAULT_PET_IMAGE),
            is_default: true,
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

/// 恢复默认形象（内置默认图片）。
#[tauri::command]
pub async fn reset_pet_image(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let store = pet_state(&app).ok_or_else(|| "app state not ready".to_string())?;
        apply_appearance(&app, &store, None)
    })
    .await
    .map_err(|error| error.to_string())?
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
    fn ring_resize_keeps_figure_centered() {
        // 展开：窗口向左上扩（宽度对称、高度向下扩展），本体保持不动。
        assert_eq!(ring_resize_delta(true), (-95.0, -80.0));
        assert_eq!(ring_resize_delta(false), (95.0, 80.0));
    }

    #[test]
    fn anchors_cursor_to_figure_center() {
        // 锚点 = 本体中心：窗口左上角 = 点击坐标 − (105, 155)×缩放。
        assert_eq!(ANCHOR_X, 105.0);
        assert_eq!(ANCHOR_Y, 155.0);
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
                image: Some("custom-1.png".into()),
            })
            .unwrap();
        assert_eq!(
            store.load(),
            PetState {
                x: -192,
                y: 88,
                image: Some("custom-1.png".into()),
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
        assert!(store.asset_path("custom-1.png").is_some());
        assert!(store.asset_path("../settings.json").is_none());
        assert!(store.asset_path("a\\b.svg").is_none());
        assert!(store.asset_path("C:\\evil.svg").is_none());
        assert!(store.asset_path(".hidden").is_none());
        assert!(store.asset_path("").is_none());
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
}
