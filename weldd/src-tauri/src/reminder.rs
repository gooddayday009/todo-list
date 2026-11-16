//! 提醒调度器:内存循环 + 30s 兜底校验,覆盖系统睡眠与时钟漂移。
//!
//! 调度线程对数据库文件持有独立连接,`sleep` 到最近一个到期点(上限 30s),
//! 醒来弹出居中提醒卡片窗 + `reminder://due` 事件,并广播 `tasks://changed`
//! 让前端按 remind_fired_at 显示持久的"过期未处理"岛提示(处理完才消失);
//! 任务增删改时命令侧调用 `invalidate()` 打断睡眠立即重算。启动时先补发
//! "错过的提醒"摘要。
//!
//! 提醒走自绘全屏窗口而非系统 Toast:未打包安装的应用没有注册 AUMID,
//! Windows 会把 Toast 静默丢弃(用户实测"到期不弹窗"的根因)。

use std::path::PathBuf;
use std::sync::{mpsc, OnceLock};
use std::thread;
use std::time::Duration;

use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::db;

/// 命令侧通知调度器"任务集合变了"的通道
static INVALIDATE_TX: OnceLock<mpsc::Sender<()>> = OnceLock::new();

pub fn invalidate() {
    if let Some(tx) = INVALIDATE_TX.get() {
        let _ = tx.send(());
    }
}

const FALLBACK_POLL: Duration = Duration::from_secs(30);

/// 到期对话框窗口的固定 label(capabilities 按它授权,前端按它分流)
pub const DIALOG_LABEL: &str = "reminder";

pub fn spawn(app: AppHandle, db_path: PathBuf) {
    let (tx, rx) = mpsc::channel::<()>();
    let _ = INVALIDATE_TX.set(tx);
    thread::spawn(move || {
        let conn = match Connection::open(&db_path) {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("[weldd] 提醒线程打开数据库失败: {e}");
                return;
            }
        };
        if let Err(e) = conn.busy_timeout(Duration::from_millis(3000)) {
            eprintln!("[weldd] 提醒线程设置 busy_timeout 失败: {e}");
        }

        // 启动补发:一次性摘要所有过期未触发的提醒(覆盖应用被杀/睡眠期间错过的)
        catch_up_missed(&app, &conn);

        let mut last_date = db::today();
        loop {
            let date = db::today();
            match db::rollover_tasks(&conn, &date) {
                Ok(changed) => {
                    // 即使没有待办顺延,跨天也要刷新已完成和预定今日的任务。
                    if changed > 0 || date != last_date {
                        let _ = app.emit("tasks://changed", ());
                        if changed > 0 {
                            invalidate();
                        }
                    }
                    last_date = date;
                }
                Err(e) => eprintln!("[weldd] 顺延未完成任务失败: {e}"),
            }
            let wait = match db::next_reminder_ms(&conn) {
                Ok(Some(next)) => Duration::from_millis((next - db::now_ms()).max(0) as u64)
                    .min(FALLBACK_POLL),
                _ => FALLBACK_POLL,
            };
            match rx.recv_timeout(wait) {
                // 超时醒来:既可能是到期,也可能是兜底校验,统一扫一遍
                Err(mpsc::RecvTimeoutError::Timeout) => fire_due(&app, &conn),
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Ok(()) => {} // 任务变动,重算等待
            }
        }
    });
}

/// 弹出(或重建)到期提醒卡片:居中小窗,列出到期任务标题 + 逐项"完成"
/// (用户拍板:列表要,时间等详细内容不要)。列表数据由前端 invoke
/// `list_due_tasks` 拉取,这里只负责窗口生命周期;已开则先关再建,
/// 下批到期时新窗口重新拉列表。
fn show_dialog(app: &AppHandle, missed: bool) {
    let app = app.clone();
    // 窗口创建必须在主线程;调度线程只通过 run_on_main_thread 投递
    let _ = app.clone().run_on_main_thread(move || {
        if let Some(win) = app.get_webview_window(DIALOG_LABEL) {
            let _ = win.close();
        }
        let kind = if missed { "missed" } else { "due" };
        let url = format!("index.html?window=reminder&kind={kind}");
        if let Err(e) = WebviewWindowBuilder::new(&app, DIALOG_LABEL, WebviewUrl::App(url.into()))
            .title("weldd · 任务到期")
            .inner_size(400.0, 440.0)
            .center()
            .transparent(true)
            .decorations(false)
            .resizable(false)
            .maximizable(false)
            .minimizable(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .focused(true)
            .build()
        {
            eprintln!("[weldd] 创建到期弹框失败: {e}");
        }
    });
}

/// 前端"知道了"后关窗;窗口销毁,下次到期重建
pub fn close_dialog(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(DIALOG_LABEL) {
        let _ = win.close();
    }
}

fn catch_up_missed(app: &AppHandle, conn: &Connection) {
    let now = db::now_ms();
    let due = match db::due_tasks(conn, now) {
        Ok(due) => due,
        Err(e) => {
            eprintln!("[weldd] 扫描错过的提醒失败: {e}");
            return;
        }
    };
    if due.is_empty() {
        return;
    }
    crate::webhook::notify(app, due.iter().map(|t| t.title.clone()).collect(), true);
    show_dialog(app, true);
    let ids: Vec<i64> = due.iter().map(|t| t.id).collect();
    let _ = db::mark_fired(conn, &ids, now);
    // remind_fired_at 变了要广播:前端 refetch 后据此显示持久的"过期未处理"提示
    let _ = app.emit("tasks://changed", ());
    let _ = app.emit(
        "reminder://due",
        serde_json::json!({
            "kind": "missed",
            "count": due.len(),
            "title": due[0].title.clone(),
        }),
    );
}

fn fire_due(app: &AppHandle, conn: &Connection) {
    let now = db::now_ms();
    let due = match db::due_tasks(conn, now) {
        Ok(due) => due,
        Err(e) => {
            eprintln!("[weldd] 查询到期提醒失败: {e}");
            return;
        }
    };
    if due.is_empty() {
        return;
    }

    crate::webhook::notify(app, due.iter().map(|t| t.title.clone()).collect(), false);
    show_dialog(app, false);
    let ids: Vec<i64> = due.iter().map(|t| t.id).collect();
    let _ = db::mark_fired(conn, &ids, now);
    // 标记已触发后广播,让岛进入持久的过期提示态
    let _ = app.emit("tasks://changed", ());
    let _ = app.emit(
        "reminder://due",
        serde_json::json!({
            "kind": "due",
            "count": due.len(),
            "title": due[0].title.clone(),
        }),
    );
}
