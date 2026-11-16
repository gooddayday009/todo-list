//! 灵动岛窗口的形态状态机与几何动画。
//!
//! 岛是一个无边框透明窗口,在「折叠胶囊 / 展开面板 / 左右·顶部边缘停靠」几种
//! 物理尺寸间切换。折叠⇄展开保持水平中心与顶部不动(从胶囊向下生长),
//! 用 60fps 步进 set_size/set_position 模拟平滑展开动画;停靠为瞬时几何。
//! 位置与形态持久化到 store,拖动停止后按当前显示器工作区夹紧。

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow,
};
use tauri_plugin_store::StoreExt;

/// 折叠态窗口逻辑尺寸(含 10px 阴影留边,可见胶囊 44px 高)
const COLLAPSED_SIZE: (f64, f64) = (380.0, 64.0);
/// 展开态窗口逻辑尺寸
const EXPANDED_SIZE: (f64, f64) = (420.0, 580.0);
/// 停靠把手尺寸
const DOCKED_SIZE: (f64, f64) = (16.0, 100.0);
/// 顶部停靠把手尺寸(横向,贴工作区顶缘)
const TOP_DOCKED_SIZE: (f64, f64) = (100.0, 16.0);
/// 初始位置:距屏幕顶部的逻辑像素
const TOP_GAP: f64 = 8.0;
/// 从停靠把手滑出后,胶囊离屏幕边缘的逻辑留白
const EDGE_GAP: f64 = 10.0;
/// 拖动松手时,窗口边缘距工作区左右缘小于该逻辑值则自动收边停靠
const EDGE_SNAP_PX: f64 = 36.0;
/// 动画节奏:16ms/步 × 10 步 ≈ 160ms
const ANIM_STEPS: u32 = 10;
const ANIM_STEP_MS: u64 = 16;
/// Moved 事件停稳判定窗口(拖动结束检测)
const SETTLE_MS: u64 = 500;

const STORE_FILE: &str = "settings.json";

/// 允许前端读写的轻量 UI 配置键(白名单,防止任意写 store)
pub const UI_SETTING_KEYS: &[&str] = &["ui_opacity", "top_dock"];

pub fn store_file() -> &'static str {
    STORE_FILE
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum IslandMode {
    #[default]
    Collapsed,
    Expanded,
    DockedLeft,
    DockedRight,
    DockedTop,
}

impl IslandMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "collapsed" => Some(Self::Collapsed),
            "expanded" => Some(Self::Expanded),
            "docked-left" => Some(Self::DockedLeft),
            "docked-right" => Some(Self::DockedRight),
            "docked-top" => Some(Self::DockedTop),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Collapsed => "collapsed",
            Self::Expanded => "expanded",
            Self::DockedLeft => "docked-left",
            Self::DockedRight => "docked-right",
            Self::DockedTop => "docked-top",
        }
    }

    fn target_size(self) -> (f64, f64) {
        match self {
            Self::Collapsed => COLLAPSED_SIZE,
            Self::Expanded => EXPANDED_SIZE,
            Self::DockedLeft | Self::DockedRight => DOCKED_SIZE,
            Self::DockedTop => TOP_DOCKED_SIZE,
        }
    }
}

/// 当前形态。Rust 侧是单一事实来源,前端通过 island://mode 事件同步。
#[derive(Default)]
pub struct IslandState(Mutex<IslandMode>);

/// 动画代际号:新的形态切换会让进行中的动画立即失效,避免 resize 抖动
static ANIM_GENERATION: AtomicU32 = AtomicU32::new(0);
/// 动画期间窗口的 Moved 事件来自我们自己 set_position,不参与拖动跟踪
static ANIMATING: AtomicBool = AtomicBool::new(false);
/// 用户按住胶囊拖动过(drag_island 触发)。只有真拖动后的停稳才做
/// 贴边吸附与位置保存,避免程序性移动(把手滑出、停靠)被误判成拖动
static USER_DRAGGED: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
struct MoveTracker {
    last: Option<(i32, i32)>,
    changed_at: Option<Instant>,
    settled: bool,
}

static TRACKER: OnceLock<Mutex<MoveTracker>> = OnceLock::new();

fn tracker() -> &'static Mutex<MoveTracker> {
    TRACKER.get_or_init(|| Mutex::new(MoveTracker::default()))
}

/// 最后一次写入的窗口几何(目标值)。Windows 的 set_position/set_size
/// 是异步的,写完立刻回读会拿到旧值,所以锚点一律取这里的记录。
/// (h 目前只作记录,锚点计算只用 x/y/w)
#[derive(Clone, Copy)]
struct Geom {
    x: i32,
    y: i32,
    w: f64,
    #[allow(dead_code)]
    h: f64,
}

static LAST_GEOM: OnceLock<Mutex<Option<Geom>>> = OnceLock::new();

fn set_last_geom(g: Geom) {
    *LAST_GEOM
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap() = Some(g);
}

fn last_geom(window: &WebviewWindow) -> Geom {
    let cached = LAST_GEOM
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap();
    if let Some(g) = *cached {
        return g;
    }
    let pos = window.outer_position().unwrap_or_default();
    let size = window.outer_size().unwrap_or_default();
    Geom {
        x: pos.x,
        y: pos.y,
        w: size.width as f64,
        h: size.height as f64,
    }
}

fn update_last_pos(x: i32, y: i32) {
    let mut cached = LAST_GEOM
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap();
    match *cached {
        Some(g) => *cached = Some(Geom { x, y, ..g }),
        None => *cached = Some(Geom { x, y, w: 0.0, h: 0.0 }),
    }
}

fn island_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("island")
}

struct WorkArea {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

/// 窗口所在显示器的工作区(去掉任务栏)
fn work_area_of(window: &WebviewWindow) -> WorkArea {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    match monitor {
        Some(m) => {
            let work = m.work_area();
            WorkArea {
                x: work.position.x,
                y: work.position.y,
                width: work.size.width as i32,
                height: work.size.height as i32,
            }
        }
        None => WorkArea {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
        },
    }
}

fn save_setting(app: &AppHandle, key: &str, value: impl Into<serde_json::Value>) {
    let Ok(store) = app.store(STORE_FILE) else {
        return;
    };
    store.set(key, value);
    let _ = store.save();
}

/// 拖动松手靠近屏幕顶部时是否自动收缩为顶部把手(设置页开关,默认开)
fn top_dock_enabled(app: &AppHandle) -> bool {
    app.store(STORE_FILE)
        .ok()
        .and_then(|s| s.get("top_dock"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

#[tauri::command]
pub fn get_island_mode(state: State<'_, IslandState>) -> String {
    state.0.lock().unwrap().as_str().to_owned()
}

#[tauri::command]
pub fn set_island_mode(
    app: AppHandle,
    state: State<'_, IslandState>,
    mode: String,
) -> Result<(), String> {
    transition(&app, &state, mode)
}

/// 形态切换公共路径(命令 / 托盘 / 快捷键共用)
fn transition(app: &AppHandle, state: &State<'_, IslandState>, mode: String) -> Result<(), String> {
    let next = IslandMode::parse(&mode).ok_or_else(|| format!("未知形态: {mode}"))?;
    let mut current = state.0.lock().unwrap();
    if *current == next {
        return Ok(());
    }
    let prev = *current;
    *current = next;
    drop(current);

    apply_mode(app, prev, next, true).map_err(|e| e.to_string())?;
    save_setting(app, "mode", next.as_str());
    app.emit("island://mode", next.as_str())
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 供托盘等无 State 上下文的调用方使用
pub fn set_mode_str(app: &AppHandle, mode: &str) -> Result<(), String> {
    let state = app.state::<IslandState>();
    transition(app, &state, mode.to_string())
}

fn apply_mode(
    app: &AppHandle,
    prev: IslandMode,
    next: IslandMode,
    animated: bool,
) -> tauri::Result<()> {
    let Some(window) = island_window(app) else {
        return Ok(());
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    // 把手与胶囊的布局不同,不能在把手的小窗口里逐帧挤压完整胶囊。
    let animated = animated && matches!(prev, IslandMode::Collapsed | IslandMode::Expanded);

    match next {
        IslandMode::DockedLeft | IslandMode::DockedRight | IslandMode::DockedTop => {
            // 停靠:瞬时几何;左右贴工作区左右缘垂直居中,顶部贴顶缘停在原水平位置
            bump_generation();
            ANIMATING.store(false, Ordering::SeqCst);
            let work = work_area_of(&window);
            let (w, h, x, y) = if next == IslandMode::DockedTop {
                let (w, h) = (TOP_DOCKED_SIZE.0 * scale, TOP_DOCKED_SIZE.1 * scale);
                // 按当前几何的中心停住(不回读窗口,取 LAST_GEOM 目标值)
                let g = last_geom(&window);
                let min_x = work.x;
                let max_x = (work.x + work.width - w as i32).max(work.x);
                let x = ((g.x as f64 + g.w / 2.0 - w / 2.0).round() as i32).clamp(min_x, max_x);
                (w, h, x, work.y)
            } else {
                let (w, h) = (DOCKED_SIZE.0 * scale, DOCKED_SIZE.1 * scale);
                let y = work.y + (work.height - h as i32) / 2;
                let x = if next == IslandMode::DockedLeft {
                    work.x
                } else {
                    work.x + work.width - w as i32
                };
                (w, h, x, y)
            };
            window.set_size(PhysicalSize::new(w.max(16.0) as u32, h.max(16.0) as u32))?;
            window.set_position(PhysicalPosition::new(x, y))?;
            set_last_geom(Geom {
                x,
                y,
                w,
                h,
            });
            let _ = window.set_focusable(false);
        }
        IslandMode::Collapsed | IslandMode::Expanded => {
            // 锚点:水平中心与顶部保持不动(取最后写入的几何,不信异步回读);
            // 从停靠滑出时锚点贴边缘
            let g = last_geom(&window);
            let (mut center_x, mut top) = (g.x as f64 + g.w / 2.0, g.y as f64);
            if prev == IslandMode::DockedTop {
                // 从顶部把手滑出:水平停在把手处,顶部回常规 TOP_GAP
                let work = work_area_of(&window);
                let target_w = next.target_size().0 * scale;
                let min_c = work.x as f64 + target_w / 2.0;
                let max_c = (work.x + work.width) as f64 - target_w / 2.0;
                center_x = (g.x as f64 + g.w / 2.0).clamp(min_c.min(max_c), max_c.max(min_c));
                top = work.y as f64 + TOP_GAP * scale;
            } else if matches!(prev, IslandMode::DockedLeft | IslandMode::DockedRight) {
                let work = work_area_of(&window);
                let target_w = next.target_size().0 * scale;
                let margin = EDGE_GAP * scale + if next == IslandMode::Expanded { 10.0 } else { 0.0 };
                center_x = if prev == IslandMode::DockedLeft {
                    work.x as f64 + margin + target_w / 2.0
                } else {
                    (work.x + work.width) as f64 - margin - target_w / 2.0
                };
            }

            // from 取窗口当前实际尺寸(仅影响动画首帧,读到略旧值无碍);
            // 目标几何与锚点全部来自上面的计算,不回读
            let (from_w, from_h) = {
                let size = window.outer_size()?;
                (size.width as f64, size.height as f64)
            };
            let (to_w, to_h) = {
                let (w, h) = next.target_size();
                (w * scale, h * scale)
            };

            if animated {
                animate_resize(&window, from_w, from_h, to_w, to_h, center_x, top);
            } else {
                bump_generation();
                ANIMATING.store(false, Ordering::SeqCst);
                window.set_size(PhysicalSize::new(
                    to_w.max(16.0) as u32,
                    to_h.max(16.0) as u32,
                ))?;
                window.set_position(PhysicalPosition::new(
                    (center_x - to_w / 2.0).round() as i32,
                    top.round() as i32,
                ))?;
            }
            set_last_geom(Geom {
                x: (center_x - to_w / 2.0).round() as i32,
                y: top.round() as i32,
                w: to_w,
                h: to_h,
            });
            if !animated {
                note_moved_position(&window);
            }
            let _ = window.set_focusable(next == IslandMode::Expanded);
        }
    }
    Ok(())
}

fn bump_generation() -> u32 {
    ANIM_GENERATION.fetch_add(1, Ordering::SeqCst) + 1
}

fn animate_resize(
    window: &WebviewWindow,
    from_w: f64,
    from_h: f64,
    to_w: f64,
    to_h: f64,
    center_x: f64,
    top: f64,
) {
    let generation = bump_generation();
    ANIMATING.store(true, Ordering::SeqCst);
    // 动画开始即登记目标几何:期间若再次切换,锚点取目标值而非中途值
    set_last_geom(Geom {
        x: (center_x - to_w / 2.0).round() as i32,
        y: top.round() as i32,
        w: to_w,
        h: to_h,
    });
    let window = window.clone();
    thread::spawn(move || {
        for step in 1..=ANIM_STEPS {
            if ANIM_GENERATION.load(Ordering::SeqCst) != generation {
                return; // 已被更新的切换取代
            }
            let t = step as f64 / ANIM_STEPS as f64;
            let eased = 1.0 - (1.0 - t).powi(3); // easeOutCubic
            let w = from_w + (to_w - from_w) * eased;
            let h = from_h + (to_h - from_h) * eased;
            let x = center_x - w / 2.0;
            let frame_window = window.clone();
            // 在实际执行帧时再次检查代际,阻止队列里的旧 resize 覆盖新形态。
            let _ = window.run_on_main_thread(move || {
                if ANIM_GENERATION.load(Ordering::SeqCst) != generation {
                    return;
                }
                let _ = frame_window.set_size(PhysicalSize::new(w.max(16.0) as u32, h.max(16.0) as u32));
                let _ = frame_window.set_position(PhysicalPosition::new(x.round() as i32, top.round() as i32));
                if step == ANIM_STEPS {
                    ANIMATING.store(false, Ordering::SeqCst);
                    note_moved_position(&frame_window);
                }
            });
            thread::sleep(Duration::from_millis(ANIM_STEP_MS));
        }
    });
}

/// 把窗口当前位置记为"待停稳保存"
fn note_moved_position(window: &WebviewWindow) {
    if let Ok(pos) = window.outer_position() {
        let mut t = tracker().lock().unwrap();
        t.last = Some((pos.x, pos.y));
        t.changed_at = Some(Instant::now());
        t.settled = false;
    }
}

/// 窗口 Moved 事件回调(用户拖动期间持续触发)
pub fn on_moved(_app: &AppHandle, pos: PhysicalPosition<i32>) {
    if ANIMATING.load(Ordering::SeqCst) {
        return;
    }
    update_last_pos(pos.x, pos.y);
    let mut t = tracker().lock().unwrap();
    t.last = Some((pos.x, pos.y));
    t.changed_at = Some(Instant::now());
    t.settled = false;
}

/// 停稳检测线程:Moved 停止超过 SETTLE_MS 视为拖动结束 →
/// 夹紧进工作区 → 贴近左右缘则收边停靠 → 否则持久化位置
fn spawn_settle_thread(app: AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(120));
        let due = {
            let mut t = tracker().lock().unwrap();
            match (t.last, t.changed_at, t.settled) {
                (Some(_pos), Some(at), false)
                    if at.elapsed() >= Duration::from_millis(SETTLE_MS) =>
                {
                    t.settled = true;
                    Some(())
                }
                _ => None,
            }
        };
        if due.is_none() {
            continue;
        }
        // 只有用户拖动引发的停稳才处理;程序性移动的几何由写入方自己负责
        if !USER_DRAGGED.swap(false, Ordering::SeqCst) {
            continue;
        }
        let Some(window) = island_window(&app) else {
            continue;
        };
        clamp_to_workarea(&window);

        // 贴边自动收边:窗口边缘离工作区左右缘不足阈值 → 收成停靠把手;
        // 顶部吸附受设置开关控制(默认位置就在顶部附近,避免误吸)
        let mode = *app.state::<IslandState>().0.lock().unwrap();
        if mode == IslandMode::Collapsed || mode == IslandMode::Expanded {
            let scale = window.scale_factor().unwrap_or(1.0);
            let snap = (EDGE_SNAP_PX * scale) as i32;
            let work = work_area_of(&window);
            if let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) {
                let near_left = pos.x - work.x <= snap;
                let near_right = work.x + work.width - (pos.x + size.width as i32) <= snap;
                let near_top = top_dock_enabled(&app) && pos.y - work.y <= snap;
                let target = if near_left {
                    Some("docked-left")
                } else if near_right {
                    Some("docked-right")
                } else if near_top {
                    Some("docked-top")
                } else {
                    None
                };
                if let Some(mode_str) = target {
                    if let Err(e) = set_mode_str(&app, mode_str) {
                        eprintln!("[weldd] 贴边停靠失败: {e}");
                    }
                    continue; // 停靠几何已写入,无需再存旧位置
                }
            }
        }

        if let Ok(pos) = window.outer_position() {
            save_setting(&app, "pos_x", pos.x);
            save_setting(&app, "pos_y", pos.y);
        }
    });
}

/// 把窗口完整夹进当前显示器工作区(多显示器拖放后按所在屏工作区回收)
fn clamp_to_workarea(window: &WebviewWindow) -> bool {
    let Ok(pos) = window.outer_position() else {
        return false;
    };
    let Ok(size) = window.outer_size() else {
        return false;
    };
    let work = work_area_of(window);
    let max_x = (work.x + work.width - size.width as i32).max(work.x);
    let max_y = (work.y + work.height - size.height as i32).max(work.y);
    let x = pos.x.clamp(work.x, max_x);
    let y = pos.y.clamp(work.y, max_y);
    if (x, y) != (pos.x, pos.y) {
        let _ = window.set_position(PhysicalPosition::new(x, y));
        true
    } else {
        false
    }
}

/// 启动初始化:恢复持久化的形态与位置(无记录则顶部居中),显示窗口。
///
/// 注意 Windows 的 set_position 是异步的,不能"设置后立刻回读"做锚点,
/// 这里所有几何都按目标值直接计算,不经过 outer_position。
pub fn init(app: &AppHandle) -> Result<(), String> {
    let Some(window) = island_window(app) else {
        return Ok(());
    };

    let (saved_mode, saved_pos) = {
        match app.store(STORE_FILE) {
            Ok(store) => {
                let mode = store
                    .get("mode")
                    .and_then(|v| v.as_str().map(|s| s.to_string()));
                let x = store.get("pos_x").and_then(|v| v.as_i64());
                let y = store.get("pos_y").and_then(|v| v.as_i64());
                (mode, x.zip(y))
            }
            Err(_) => (None, None),
        }
    };

    // 不恢复展开态(重启后一律从胶囊/停靠开始)
    let mode = IslandMode::parse(saved_mode.as_deref().unwrap_or("collapsed"))
        .filter(|m| *m != IslandMode::Expanded)
        .unwrap_or_default();

    let scale = window.scale_factor().unwrap_or(1.0);
    match mode {
        IslandMode::DockedLeft | IslandMode::DockedRight | IslandMode::DockedTop => {
            let work = work_area_of(&window);
            let (w, h, x, y) = if mode == IslandMode::DockedTop {
                let (w, h) = (TOP_DOCKED_SIZE.0 * scale, TOP_DOCKED_SIZE.1 * scale);
                let x = work.x + ((work.width - w as i32).max(0)) / 2;
                (w, h, x, work.y)
            } else {
                let (w, h) = (DOCKED_SIZE.0 * scale, DOCKED_SIZE.1 * scale);
                let y = work.y + (work.height - h as i32) / 2;
                let x = if mode == IslandMode::DockedLeft {
                    work.x
                } else {
                    work.x + work.width - w as i32
                };
                (w, h, x, y)
            };
            bump_generation();
            ANIMATING.store(false, Ordering::SeqCst);
            let _ = window.set_size(PhysicalSize::new(w.max(16.0) as u32, h.max(16.0) as u32));
            let _ = window.set_position(PhysicalPosition::new(x, y));
            set_last_geom(Geom { x, y, w, h });
            let _ = window.set_focusable(false);
        }
        _ => {
            if let Some((x, y)) = saved_pos {
                let _ = window.set_position(PhysicalPosition::new(x as i32, y as i32));
                set_last_geom(Geom { x: x as i32, y: y as i32, w: COLLAPSED_SIZE.0 * scale, h: COLLAPSED_SIZE.1 * scale });
            } else {
                place_top_center(app).map_err(|e| e.to_string())?;
            }
            // 窗口初始尺寸即折叠尺寸(tauri.conf),无需再动
        }
    }

    let state = app.state::<IslandState>();
    *state.0.lock().unwrap() = mode;
    let _ = window.set_always_on_top(true);
    let _ = window.show();

    note_moved_position(&window);
    spawn_settle_thread(app.clone());
    Ok(())
}

/// 把岛放到主显示器工作区顶部水平居中(任务栏在顶部时自动避开)
pub fn place_top_center(app: &AppHandle) -> tauri::Result<()> {
    let Some(window) = island_window(app) else {
        return Ok(());
    };
    let monitor = window
        .primary_monitor()
        .ok()
        .flatten()
        .or_else(|| window.current_monitor().ok().flatten());
    if let Some(monitor) = monitor {
        let scale = monitor.scale_factor();
        let work = monitor.work_area();
        let x = work.position.x as f64 + (work.size.width as f64 - COLLAPSED_SIZE.0 * scale) / 2.0;
        let y = work.position.y as f64 + TOP_GAP * scale;
        window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32))?;
        set_last_geom(Geom { x: x.round() as i32, y: y.round() as i32, w: COLLAPSED_SIZE.0 * scale, h: COLLAPSED_SIZE.1 * scale });
    }
    Ok(())
}

/// 重置岛位置:回到顶部居中 + 折叠态(设置面板"重置位置"用)。
/// 与 init 同理:直接按目标几何写,不回读当前位置做锚点。
pub fn reset_position(app: &AppHandle) -> Result<(), String> {
    let Some(window) = island_window(app) else {
        return Ok(());
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let work = work_area_of(&window);
    let (w, h) = (COLLAPSED_SIZE.0 * scale, COLLAPSED_SIZE.1 * scale);
    let x = work.x + (work.width - w as i32) / 2;
    let y = work.y + (TOP_GAP * scale).round() as i32;

    bump_generation();
    ANIMATING.store(false, Ordering::SeqCst);
    window
        .set_size(PhysicalSize::new(w.max(16.0) as u32, h.max(16.0) as u32))
        .map_err(|e| e.to_string())?;
    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())?;
    let _ = window.set_focusable(false);

    let state = app.state::<IslandState>();
    *state.0.lock().unwrap() = IslandMode::Collapsed;
    save_setting(app, "mode", "collapsed");
    let _ = window.show();
    note_moved_position(&window);
    let _ = app.emit("island://mode", "collapsed");
    Ok(())
}

/// 显示 / 隐藏岛(托盘与 Alt+Q 共用)
pub fn toggle_visible(app: &AppHandle) {
    let Some(window) = island_window(app) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        show_island(app);
    }
}

/// 显示岛并恢复置顶(二次启动唤起 / 托盘 / 设置入口共用)
pub fn show_island(app: &AppHandle) {
    let Some(window) = island_window(app) else {
        return;
    };
    let _ = window.show();
    let _ = window.set_always_on_top(true);
}

/// 仅隐藏窗口(保留全部状态;托盘/Alt+Q 可再显示)。注意不能走
/// 前端 window.close(),那会销毁窗口。
#[tauri::command]
pub fn hide_island(app: AppHandle) -> Result<(), String> {
    let window = island_window(&app).ok_or("找不到岛窗口")?;
    window.hide().map_err(|e| e.to_string())
}

/// Alt+N 快速添加:展开面板并聚焦输入框(前端监听 island://quick-add)
pub fn quick_add(app: &AppHandle) {
    let Some(window) = island_window(app) else {
        return;
    };
    show_island(app);
    let state = app.state::<IslandState>();
    let _ = transition(app, &state, "expanded".to_string());
    let _ = window.set_focusable(true);
    let _ = window.set_focus();
    let _ = app.emit("island://quick-add", ());
}

/// 托盘"设置":展开面板并打开设置页
pub fn open_settings(app: &AppHandle) {
    show_island(app);
    let state = app.state::<IslandState>();
    let _ = transition(app, &state, "expanded".to_string());
    if let Some(window) = island_window(app) {
        let _ = window.set_focusable(true);
    }
    let _ = app.emit("island://open-settings", ());
}

/// 进入系统拖动模式(胶囊任意空白处按下时由前端调用)
#[tauri::command]
pub fn drag_island(app: AppHandle) -> Result<(), String> {
    let window = island_window(&app).ok_or("找不到岛窗口")?;
    USER_DRAGGED.store(true, Ordering::SeqCst);
    window.start_dragging().map_err(|e| e.to_string())
}
