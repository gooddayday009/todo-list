//! 暴露给前端的命令:任务 CRUD + 设置(自启等)。
//!
//! 所有会改变任务集合的命令执行后广播 `tasks://changed` 并打断提醒调度器
//! 的睡眠,让它立即按新数据重算下一个到期点。

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_store::StoreExt;

use crate::db::{self, Db, Task};
use crate::island;
use crate::reminder;
use crate::tray;

fn broadcast_changed(app: &AppHandle) {
    let _ = app.emit("tasks://changed", ());
    reminder::invalidate();
}

#[tauri::command]
pub fn list_today_tasks(app: AppHandle, db: State<'_, Db>) -> Result<Vec<Task>, String> {
    let conn = db.conn.lock().unwrap();
    let date = db::today();
    let changed = db::rollover_tasks(&conn, &date).map_err(|e| e.to_string())?;
    let tasks = db::today_tasks(&conn, &date).map_err(|e| e.to_string())?;
    drop(conn);
    if changed > 0 {
        broadcast_changed(&app);
    }
    Ok(tasks)
}

/// 历史报表:某月每日完成/总数统计(month 形如 "2026-09",只读不广播)
#[tauri::command]
pub fn list_month_stats(db: State<'_, Db>, month: String) -> Result<Vec<db::DayStat>, String> {
    let conn = db.conn.lock().unwrap();
    db::month_stats(&conn, &month).map_err(|e| e.to_string())
}

/// 历史明细:某天全部任务(date 形如 "2026-09-08",只读不广播)
#[tauri::command]
pub fn list_day_tasks(db: State<'_, Db>, date: String) -> Result<Vec<Task>, String> {
    let conn = db.conn.lock().unwrap();
    db::day_tasks(&conn, &date).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_task(
    app: AppHandle,
    db: State<'_, Db>,
    title: String,
    remind_at: Option<i64>,
) -> Result<Task, String> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("任务标题不能为空".into());
    }
    // 归属日跟提醒日走:"明天 14:30" 的任务归明天清单,当天才可见
    let date = remind_at
        .map(db::date_of_ms)
        .unwrap_or_else(db::today);
    let task = {
        let conn = db.conn.lock().unwrap();
        db::create_task(&conn, &title, remind_at, &date).map_err(|e| e.to_string())?
    };
    broadcast_changed(&app);
    Ok(task)
}

#[tauri::command]
pub fn update_task_title(
    app: AppHandle,
    db: State<'_, Db>,
    id: i64,
    title: String,
) -> Result<(), String> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("任务标题不能为空".into());
    }
    {
        let conn = db.conn.lock().unwrap();
        db::update_task_title(&conn, id, &title).map_err(|e| e.to_string())?;
    }
    broadcast_changed(&app);
    Ok(())
}

/// 正文存的是前端白名单净化后的富文本 HTML;255 字纯文本上限在编辑器里执行,
/// 这里只按原始长度挡异常输入(带标记后合法正文也会超过 255 字节)
#[tauri::command]
pub fn update_task_note(app: AppHandle, db: State<'_, Db>, id: i64, note: String) -> Result<(), String> {
    if note.len() > 8192 {
        return Err("正文过长".into());
    }
    {
        let conn = db.conn.lock().unwrap();
        db::update_task_note(&conn, id, &note).map_err(|e| e.to_string())?;
    }
    broadcast_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn set_task_status(app: AppHandle, db: State<'_, Db>, id: i64, done: bool) -> Result<(), String> {
    {
        let conn = db.conn.lock().unwrap();
        db::set_task_status(&conn, id, done).map_err(|e| e.to_string())?;
    }
    broadcast_changed(&app);
    Ok(())
}

/// remind_at 传 null 表示清除提醒。设提醒时归属日同步挪到提醒日,
/// 防止提醒响了任务却不在那天的清单里;清除提醒时归属日不动
#[tauri::command]
pub fn set_task_reminder(
    app: AppHandle,
    db: State<'_, Db>,
    id: i64,
    remind_at: Option<i64>,
) -> Result<(), String> {
    let date = remind_at
        .map(db::date_of_ms)
        .unwrap_or_default();
    {
        let conn = db.conn.lock().unwrap();
        db::set_task_reminder(&conn, id, remind_at, &date).map_err(|e| e.to_string())?;
    }
    broadcast_changed(&app);
    Ok(())
}

/// 到期弹框的数据源:全部已过期未完成任务(含已触发过提醒的,
/// 与 mark_fired 的竞态见 db::overdue_tasks 注释;只读不广播)
#[tauri::command]
pub fn list_due_tasks(db: State<'_, Db>) -> Result<Vec<Task>, String> {
    let conn = db.conn.lock().unwrap();
    db::overdue_tasks(&conn, db::now_ms()).map_err(|e| e.to_string())
}

/// 关闭到期提醒弹框(Esc / "知道了"都走这里;窗口销毁,下次到期重建)
#[tauri::command]
pub fn close_reminder_dialog(app: AppHandle) {
    reminder::close_dialog(&app);
}

#[tauri::command]
pub fn delete_task(app: AppHandle, db: State<'_, Db>, id: i64) -> Result<(), String> {
    {
        let conn = db.conn.lock().unwrap();
        db::delete_task(&conn, id).map_err(|e| e.to_string())?;
    }
    broadcast_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> Result<bool, String> {
    app.autolaunch()
        .is_enabled()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    let autolaunch = app.autolaunch();
    if enabled {
        autolaunch.enable().map_err(|e| e.to_string())?;
    } else {
        autolaunch.disable().map_err(|e| e.to_string())?;
    }
    tray::sync_autostart_checked(&app, enabled);
    Ok(())
}

#[tauri::command]
pub fn reset_island_position(app: AppHandle) -> Result<(), String> {
    island::reset_position(&app)
}

/// 打开设置页:与托盘"设置"走同一路径(island::open_settings),
/// 供胶囊右键菜单调用
#[tauri::command]
pub fn open_island_settings(app: AppHandle) {
    island::open_settings(&app);
}

/// 读轻量 UI 配置(透明度等),与岛位置同存 settings.json
#[tauri::command]
pub fn get_ui_setting(app: AppHandle, key: String) -> Result<Option<serde_json::Value>, String> {
    if !island::UI_SETTING_KEYS.contains(&key.as_str()) {
        return Err(format!("未开放的配置键: {key}"));
    }
    let store = app
        .store(island::store_file())
        .map_err(|e| e.to_string())?;
    Ok(store.get(&key))
}

#[tauri::command]
pub fn set_ui_setting(
    app: AppHandle,
    key: String,
    value: serde_json::Value,
) -> Result<(), String> {
    if !island::UI_SETTING_KEYS.contains(&key.as_str()) {
        return Err(format!("未开放的配置键: {key}"));
    }
    let store = app
        .store(island::store_file())
        .map_err(|e| e.to_string())?;
    store.set(&key, value);
    let _ = store.save();
    Ok(())
}

// ---------- 数据导出 / 导入 ----------

/// 备份文件头:导入时校验,防误选无关 JSON
const EXPORT_FORMAT: &str = "weldd-export";
const EXPORT_VERSION: u32 = 1;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportFile<'a> {
    format: &'a str,
    version: u32,
    exported_at: i64,
    tasks: &'a [Task],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportFile {
    format: String,
    version: u32,
    tasks: Vec<Task>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub count: usize,
    pub path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: usize,
    pub skipped: usize,
}

/// 导出全部任务(含历史)为 JSON。用户取消对话框时返回 null。
/// 对话框用 blocking API,命令跑在非主线程,不会卡事件循环。
#[tauri::command]
pub fn export_tasks(app: AppHandle, db: State<'_, Db>) -> Result<Option<ExportResult>, String> {
    let default_name =
        format!("weldd-backup-{}.json", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    let picked = app
        .dialog()
        .file()
        .add_filter("weldd 备份 (JSON)", &["json"])
        .set_file_name(&default_name)
        .blocking_save_file();
    let Some(file_path) = picked else {
        return Ok(None);
    };
    let path = file_path.into_path().map_err(|e| e.to_string())?;

    let tasks = {
        let conn = db.conn.lock().unwrap();
        db::all_tasks(&conn).map_err(|e| e.to_string())?
    };
    let file = ExportFile {
        format: EXPORT_FORMAT,
        version: EXPORT_VERSION,
        exported_at: db::now_ms(),
        tasks: &tasks,
    };
    let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("写入文件失败: {e}"))?;
    Ok(Some(ExportResult {
        count: tasks.len(),
        path: path.to_string_lossy().into_owned(),
    }))
}

/// 从备份 JSON 合并导入:已存在的任务(同 created_at + 标题)跳过,
/// 新任务保留提醒/完成状态/正文。用户取消对话框时返回 null。
#[tauri::command]
pub fn import_tasks(app: AppHandle, db: State<'_, Db>) -> Result<Option<ImportResult>, String> {
    let picked = app
        .dialog()
        .file()
        .add_filter("weldd 备份 (JSON)", &["json"])
        .blocking_pick_file();
    let Some(file_path) = picked else {
        return Ok(None);
    };
    let path = file_path.into_path().map_err(|e| e.to_string())?;
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("读取文件失败: {e}"))?;
    let file: ImportFile = serde_json::from_str(&raw).map_err(|_| "不是有效的 weldd 备份文件".to_string())?;
    if file.format != EXPORT_FORMAT {
        return Err("不是 weldd 导出的备份文件".into());
    }
    if file.version > EXPORT_VERSION {
        return Err("备份文件版本过新,请先升级应用".into());
    }

    let (imported, skipped) = {
        let mut conn = db.conn.lock().unwrap();
        db::import_tasks(&mut conn, &file.tasks).map_err(|e| e.to_string())?
    };
    broadcast_changed(&app);
    Ok(Some(ImportResult { imported, skipped }))
}
