//! 桌面壁纸点击监听：WH_MOUSE_LL 低级鼠标钩子 + 工作线程分类。
//!
//! 架构（保证钩子回调轻量，不拖慢全局鼠标）：
//! - 钩子线程：装钩 + 消息泵（LL 钩子要求装钩线程泵消息）。回调只记录
//!   左键状态、按位移节流，把 (消息, 坐标) 推进无界通道 —— 不做窗口
//!   分类、不做跨进程调用、不做 IPC。
//! - 工作线程：消费通道，负责窗口分类（WindowFromPoint + 类名 + 桌面
//!   层级校验 + 跨进程图标命中测试）、双击合成、状态机与事件发射。
//!   允许在这里阻塞（SendMessageTimeoutW 等），不影响鼠标全局延迟。
//!
//! 壁纸判定（三态：壁纸 / 图标 / 其他，非壁纸一律不触发）：
//! - Progman / WorkerW / SHELLDLL_DefView → 壁纸宿主；
//! - SysListView32 铺满整个桌面（点空白壁纸也返回它），必须：
//!   a) 验证父链是 SHELLDLL_DefView → Progman/WorkerW（普通应用也有
//!   同名列表控件，不能只看类名）；
//!   b) 跨进程 LVM_HITTEST 区分「点在图标上」与「点在空白壁纸上」。
//!   LVHITTESTINFO 高于 WM_USER，系统不会跨进程封送，指针必须落在
//!   目标进程（Explorer）地址空间里 —— VirtualAllocEx +
//!   WriteProcessMemory 写入，否则列表读到野指针甚至崩掉 Explorer。
//!   任何一步失败都归为「其他」，保守不触发。
//!
//! 事件语义（坐标为桌宠窗口左上角目标位置，物理像素，已锚定钳屏）：
//! - 按住左键（落在壁纸上）→ crawl：向鼠标位置爬行，按住期间持续跟随；
//! - 松开左键（无论落在哪）→ crawl-end：爬到当前目标后停住；
//! - 双击左键（落在壁纸上）→ teleport：闪现。WH_MOUSE_LL 收不到
//!   WM_LBUTTONDBLCLK（双击合成发生在钩子之后），因此按系统双击时间
//!   与距离在两次壁纸按下之间自行判定。

use std::sync::mpsc::{channel, Sender};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, OnceLock,
};
use std::time::Instant;

use serde::Serialize;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows_sys::Win32::System::Memory::{
    VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_VM_OPERATION, PROCESS_VM_WRITE,
};
use windows_sys::Win32::UI::Controls::{LVHITTESTINFO, LVM_HITTEST};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GA_PARENT, GetAncestor, GetClassNameW, GetMessageW,
    GetSystemMetrics, GetWindowThreadProcessId, MSG, MSLLHOOKSTRUCT, SendMessageTimeoutW,
    SetWindowsHookExW, SMTO_ABORTIFHUNG, SM_CXDOUBLECLK, SM_CYDOUBLECLK, TranslateMessage,
    UnhookWindowsHookEx, WindowFromPoint, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE,
};

use tauri::{AppHandle, Emitter, Manager};

use crate::pet::PET_LABEL;

/// 钩子回调里要用的应用句柄（install 幂等：只有首次写入生效）。
static HOOK_APP: OnceLock<AppHandle> = OnceLock::new();
/// 钩子与工作线程只启动一次。
static HOOK_STARTED: AtomicBool = AtomicBool::new(false);
/// 事件通道（钩子线程 → 工作线程）。无界通道 send 永不阻塞。
static QUEUE: OnceLock<Sender<RawMouse>> = OnceLock::new();
/// 左键当前是否按下（钩子线程维护，用于移动事件过滤）。
static LEFT_DOWN: AtomicBool = AtomicBool::new(false);
/// 上次转发的移动坐标（钩子线程节流用）。
static LAST_FORWARD: Mutex<(i32, i32)> = Mutex::new((i32::MIN, i32::MIN));
/// 移动事件转发节流（物理像素）。
const MOVE_FORWARD_STEP: i32 = 10;
/// 工作线程发射爬行目标的节流（物理像素）。
const MOVE_EMIT_STEP: i32 = 10;

/// 钩子线程递交给工作线程的原始鼠标事件（只带消息与坐标）。
#[derive(Clone, Copy, Debug)]
struct RawMouse {
    message: u32,
    x: i32,
    y: i32,
}

/// 发给桌宠窗口的桌面指针事件（坐标为窗口左上角目标位置）。
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "kind")]
enum DesktopPointer {
    Crawl { x: i32, y: i32 },
    CrawlEnd { x: i32, y: i32 },
    Teleport { x: i32, y: i32 },
}

/// 点击落点分类：非壁纸一律不触发，判定失败保守归为其他。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Surface {
    /// 壁纸（可触发爬行/闪现）。
    Wallpaper,
    /// 桌面图标（明确排除）。
    Icon,
    /// 应用窗口 / 任务栏 / 桌宠自身 / 判定失败 —— 不触发。
    Other,
}

/// 安装全局鼠标钩子（幂等）：重复调用只保留首个应用句柄，线程只起一次。
pub fn install(app: AppHandle) {
    if HOOK_APP.set(app).is_err() {
        return;
    }
    if HOOK_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let (sender, receiver) = channel::<RawMouse>();
    if QUEUE.set(sender).is_err() {
        return;
    }
    // 工作线程：分类 + 状态机 + 发射（允许阻塞，绝不进钩子回调）。
    let worker = std::thread::Builder::new()
        .name("desktop-pointer-worker".into())
        .spawn(move || {
            let mut state = WorkerState::default();
            for raw in receiver {
                handle_raw(HOOK_APP.get().expect("app set before worker"), raw, &mut state);
            }
        });
    // 钩子线程：LL 钩子要求装钩线程泵消息。
    let hook = std::thread::Builder::new()
        .name("desktop-mouse-hook".into())
        .spawn(|| unsafe {
            let handle = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), std::ptr::null_mut(), 0);
            if handle.is_null() {
                log::warn!("event=desktop_hook_install_failed");
                return;
            }
            let mut message: MSG = std::mem::zeroed();
            while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            UnhookWindowsHookEx(handle);
        });
    if worker.is_err() || hook.is_err() {
        log::warn!("event=desktop_hook_thread_failed");
    }
}

unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lparam as *const MSLLHOOKSTRUCT);
        let (x, y) = (info.pt.x, info.pt.y);
        match wparam as u32 {
            WM_LBUTTONDOWN => {
                LEFT_DOWN.store(true, Ordering::SeqCst);
                if let Ok(mut last) = LAST_FORWARD.lock() {
                    *last = (x, y);
                }
                forward(WM_LBUTTONDOWN, x, y);
            }
            WM_LBUTTONUP => {
                LEFT_DOWN.store(false, Ordering::SeqCst);
                if let Ok(mut last) = LAST_FORWARD.lock() {
                    *last = (i32::MIN, i32::MIN);
                }
                forward(WM_LBUTTONUP, x, y);
            }
            // 只在按住期间转发移动事件，且超过阈值才转发（节流）。
            WM_MOUSEMOVE if LEFT_DOWN.load(Ordering::SeqCst) => {
                if let Ok(mut last) = LAST_FORWARD.lock() {
                    if (x - last.0).abs() + (y - last.1).abs() >= MOVE_FORWARD_STEP {
                        *last = (x, y);
                        forward(WM_MOUSEMOVE, x, y);
                    }
                }
            }
            _ => {}
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

/// 把原始事件推给工作线程；无界通道 send 不阻塞，钩子回调保持轻快。
fn forward(message: u32, x: i32, y: i32) {
    if let Some(sender) = QUEUE.get() {
        let _ = sender.send(RawMouse { message, x, y });
    }
}

/// 工作线程状态机。
#[derive(Default)]
struct WorkerState {
    /// 左键已在壁纸按下并启动爬行（跟随移动中）。
    crawling: bool,
    /// 上一次壁纸按下（坐标 + 时间），用于合成双击。
    last_wallpaper_down: Option<((i32, i32), Instant)>,
    /// 上次发射的坐标（发射节流）。
    last_emit: (i32, i32),
}

/// 工作线程的事件处理：分类在 gratuits 线程做，允许阻塞。
fn handle_raw(app: &AppHandle, raw: RawMouse, state: &mut WorkerState) {
    match raw.message {
        WM_LBUTTONDOWN => {
            if classify(raw.x, raw.y) != Surface::Wallpaper {
                // 不在壁纸上的按下：打断爬行与双击序列。
                state.crawling = false;
                state.last_wallpaper_down = None;
                return;
            }
            let now = Instant::now();
            let double = state
                .last_wallpaper_down
                .take()
                .map(|(previous, at)| {
                    within_double_click(previous, at, (raw.x, raw.y), now)
                })
                .unwrap_or(false);
            if double {
                // 双击：闪现（不再启动爬行，前端会取消未完成的爬行）。
                state.crawling = false;
                refresh_scale(app);
                emit(app, DesktopPointer::Teleport { x: raw.x, y: raw.y });
                return;
            }
            state.last_wallpaper_down = Some(((raw.x, raw.y), now));
            state.crawling = true;
            state.last_emit = (raw.x, raw.y);
            refresh_scale(app);
            emit(app, DesktopPointer::Crawl { x: raw.x, y: raw.y });
        }
        WM_MOUSEMOVE => {
            if !state.crawling {
                return;
            }
            if (raw.x - state.last_emit.0).abs() + (raw.y - state.last_emit.1).abs()
                < MOVE_EMIT_STEP
            {
                return;
            }
            state.last_emit = (raw.x, raw.y);
            emit(app, DesktopPointer::Crawl { x: raw.x, y: raw.y });
        }
        // 松开（无论落在哪）都要结束手势：爬到当前目标后停住。
        WM_LBUTTONUP if state.crawling => {
            state.crawling = false;
            emit(app, DesktopPointer::CrawlEnd { x: raw.x, y: raw.y });
        }
        _ => {}
    }
}

/// 判断屏幕坐标是否落在桌面壁纸上（排除图标、任务栏与应用窗口）。
fn classify(x: i32, y: i32) -> Surface {
    unsafe {
        let hwnd = WindowFromPoint(POINT { x, y });
        if hwnd.is_null() {
            return Surface::Other;
        }
        let class = window_class(hwnd);
        if !is_wallpaper_class(&class) {
            return Surface::Other;
        }
        if class != "SysListView32" {
            return Surface::Wallpaper;
        }
        // 图标列表铺满整个桌面：先验证属于桌面层级（排除普通应用的
        // 同名控件），再用跨进程命中测试区分图标与空白壁纸。
        if !is_desktop_listview(hwnd) {
            return Surface::Other;
        }
        match listview_hits_icon(hwnd, POINT { x, y }) {
            Some(true) => Surface::Icon,
            Some(false) => Surface::Wallpaper,
            None => Surface::Other, // 命中测试失败：保守不触发。
        }
    }
}

/// 壁纸候选类名（SysListView32 还需桌面层级校验与图标命中测试）。
fn is_wallpaper_class(class: &str) -> bool {
    matches!(
        class,
        "Progman" | "WorkerW" | "SHELLDLL_DefView" | "SysListView32"
    )
}

/// 验证 SysListView32 属于桌面层级：SHELLDLL_DefView → Progman/WorkerW。
/// 普通应用也有同名列表控件，不能只看类名。
fn is_desktop_listview(hwnd: windows_sys::Win32::Foundation::HWND) -> bool {
    unsafe {
        let parent = GetAncestor(hwnd, GA_PARENT);
        if parent.is_null() || window_class(parent) != "SHELLDLL_DefView" {
            return false;
        }
        let grand = GetAncestor(parent, GA_PARENT);
        !grand.is_null() && matches!(window_class(grand).as_str(), "Progman" | "WorkerW")
    }
}

/// 跨进程对桌面图标列表做 LVM_HITTEST：命中图标返回 Some(true)，
/// 空白壁纸返回 Some(false)，任何失败返回 None（保守不触发）。
fn listview_hits_icon(hwnd: windows_sys::Win32::Foundation::HWND, pt: POINT) -> Option<bool> {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_VM_OPERATION | PROCESS_VM_WRITE, 0, pid);
        if process.is_null() {
            return None;
        }
        let hit = remote_hit_test(process, hwnd, pt);
        CloseHandle(process);
        hit
    }
}

/// 在目标进程里分配一块内存写入 LVHITTESTINFO，执行带超时的
/// LVM_HITTEST，读取返回的命中索引（≥ 0 为图标项，-1 为空白）。
fn remote_hit_test(process: HANDLE, hwnd: windows_sys::Win32::Foundation::HWND, pt: POINT) -> Option<bool> {
    unsafe {
        let mut info: LVHITTESTINFO = std::mem::zeroed();
        info.pt = pt;
        if ScreenToClient(hwnd, &mut info.pt) == 0 {
            return None;
        }
        let size = std::mem::size_of::<LVHITTESTINFO>();
        let remote = VirtualAllocEx(
            process,
            std::ptr::null(),
            size,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        );
        if remote.is_null() {
            return None;
        }
        let mut written = 0usize;
        let written_ok = WriteProcessMemory(
            process,
            remote,
            &info as *const LVHITTESTINFO as *const core::ffi::c_void,
            size,
            &mut written,
        );
        let mut hit_index = 0usize;
        let sent = written_ok != 0
            && SendMessageTimeoutW(
                hwnd,
                LVM_HITTEST,
                0,
                remote as isize,
                SMTO_ABORTIFHUNG,
                200,
                &mut hit_index,
            ) != 0;
        VirtualFreeEx(process, remote, 0, MEM_RELEASE);
        if !sent {
            return None;
        }
        Some((hit_index as isize) >= 0)
    }
}

/// 读取窗口类名；读取失败返回空串（必然不匹配壁纸）。
fn window_class(hwnd: windows_sys::Win32::Foundation::HWND) -> String {
    unsafe {
        let mut buffer = [0u16; 32];
        let len = GetClassNameW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
        if len <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buffer[..(len as usize).min(buffer.len())])
    }
}

/// 两次壁纸按下是否构成本地合成的双击：系统双击时间内、双轴均在
/// 系统双击矩形容差内（WH_MOUSE_LL 收不到 WM_LBUTTONDBLCLK）。
fn within_double_click(
    previous: (i32, i32),
    previous_at: Instant,
    current: (i32, i32),
    now: Instant,
) -> bool {
    unsafe {
        let interval = (GetDoubleClickTime() as u128).max(100);
        let tolerance = (
            GetSystemMetrics(SM_CXDOUBLECLK).max(2),
            GetSystemMetrics(SM_CYDOUBLECLK).max(2),
        );
        is_within_double_click(
            previous,
            now.duration_since(previous_at).as_millis(),
            current,
            interval,
            tolerance,
        )
    }
}

/// 纯时间/几何判定（可测）：时间窗内 + 双轴均在容差内。
fn is_within_double_click(
    previous: (i32, i32),
    elapsed_ms: u128,
    current: (i32, i32),
    interval_ms: u128,
    tolerance: (i32, i32),
) -> bool {
    elapsed_ms <= interval_ms
        && (current.0 - previous.0).abs() <= tolerance.0
        && (current.1 - previous.1).abs() <= tolerance.1
}

/// 手势起点刷新缓存的 DPI 缩放（爬行途中沿用，避免每帧跨线程查询）。
fn refresh_scale(app: &AppHandle) {
    if let Some(pet) = app.get_webview_window(PET_LABEL) {
        if let Ok(scale) = pet.scale_factor() {
            crate::pet::store_pet_scale(scale);
        }
    }
}

/// 把壁纸点击换算成桌宠窗口目标位置并发给桌宠窗口。
fn emit(app: &AppHandle, event: DesktopPointer) {
    let (raw_x, raw_y) = match &event {
        DesktopPointer::Crawl { x, y }
        | DesktopPointer::CrawlEnd { x, y }
        | DesktopPointer::Teleport { x, y } => (*x, *y),
    };
    let (x, y) = crate::pet::anchored_pet_position(app, raw_x, raw_y);
    let event = match event {
        DesktopPointer::Crawl { .. } => DesktopPointer::Crawl { x, y },
        DesktopPointer::CrawlEnd { .. } => DesktopPointer::CrawlEnd { x, y },
        DesktopPointer::Teleport { .. } => DesktopPointer::Teleport { x, y },
    };
    // 桌宠窗口未打开时无收件人，静默即可。
    let _ = app.emit_to(PET_LABEL, "desktop-pointer", event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallpaper_host_classes_only() {
        assert!(is_wallpaper_class("Progman"));
        assert!(is_wallpaper_class("WorkerW"));
        // DefView 是图标列表的宿主，SysListView32 是铺满桌面的图标列表
        //（空白壁纸也返回它，需再经桌面层级校验与图标命中测试区分）。
        assert!(is_wallpaper_class("SHELLDLL_DefView"));
        assert!(is_wallpaper_class("SysListView32"));
        // 任务栏 / 普通应用窗口 / 桌宠自身都不是壁纸。
        assert!(!is_wallpaper_class("Shell_TrayWnd"));
        assert!(!is_wallpaper_class("Chrome_WidgetWin_1"));
        assert!(!is_wallpaper_class(""));
    }

    #[test]
    fn double_click_needs_time_and_proximity() {
        // 500ms 窗口、4px 容差：同点快速连按命中；超时或位移过大不算。
        assert!(is_within_double_click((100, 200), 120, (102, 198), 500, (4, 4)));
        assert!(!is_within_double_click((100, 200), 600, (100, 200), 500, (4, 4)));
        assert!(!is_within_double_click((100, 200), 120, (110, 200), 500, (4, 4)));
        assert!(!is_within_double_click((100, 200), 120, (100, 210), 500, (4, 4)));
    }
}
