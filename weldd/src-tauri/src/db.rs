//! SQLite(rusqlite):迁移 + 任务 CRUD。
//!
//! 数据库是唯一数据入口:命令走 `State<Db>` 里的主连接;提醒调度线程在
//! `reminder.rs` 里对同一文件自开一条连接(WAL 模式下读写不互斥)。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

pub struct Db {
    pub conn: Mutex<Connection>,
    path: PathBuf,
}

impl Db {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// 本地时区的今日日期(任务归属日用本地日界)
pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// 毫秒时间戳 → 本地日期串。任务归属日跟提醒日走:
/// "明天 14:30" 添加的任务归明天清单,当天才可见
pub fn date_of_ms(ms: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(ms)
        .single()
        .unwrap_or_else(chrono::Local::now)
        .format("%Y-%m-%d")
        .to_string()
}

pub fn now_ms() -> i64 {
    chrono::Local::now().timestamp_millis()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub note: String,
    pub status: i64,
    pub date: String,
    pub remind_at: Option<i64>,
    pub remind_fired_at: Option<i64>,
    pub sort_order: i64,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

// list/focus_minutes 列留在表里(旧库兼容,SQLite 删列需重建表)但不再进应用层
const TASK_COLS: &str = "id, title, note, status, date, remind_at, remind_fired_at, \
                         sort_order, created_at, completed_at";

fn row_to_task(row: &Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        title: row.get(1)?,
        note: row.get(2)?,
        status: row.get(3)?,
        date: row.get(4)?,
        remind_at: row.get(5)?,
        remind_fired_at: row.get(6)?,
        sort_order: row.get(7)?,
        created_at: row.get(8)?,
        completed_at: row.get(9)?,
    })
}

const MIGRATION: &str = "
CREATE TABLE IF NOT EXISTS tasks (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  title       TEXT NOT NULL,
  note        TEXT DEFAULT '',
  status      INTEGER NOT NULL DEFAULT 0,
  list        TEXT NOT NULL DEFAULT 'today',
  date        TEXT NOT NULL,
  remind_at   INTEGER,
  remind_fired_at INTEGER,
  focus_minutes INTEGER NOT NULL DEFAULT 25,
  sort_order  INTEGER NOT NULL DEFAULT 0,
  created_at  INTEGER NOT NULL,
  completed_at INTEGER
);
CREATE TABLE IF NOT EXISTS focus_sessions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  started_at INTEGER NOT NULL,
  ended_at INTEGER,
  planned_minutes INTEGER NOT NULL,
  completed INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_tasks_date ON tasks(date);
CREATE INDEX IF NOT EXISTS idx_tasks_pending_remind
  ON tasks(remind_at) WHERE remind_fired_at IS NULL AND status = 0;
";

fn open_conn(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_millis(3000))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(MIGRATION)?;
    // 存量数据修正:早期创建任务一律记当天,"明天"任务到了目标日不出现。
    // 旧提醒最早归今天,避免重启把已顺延任务退回过去;已完成的不动以保历史统计。
    conn.execute(
        "UPDATE tasks
         SET date = MAX(?1, strftime('%Y-%m-%d', remind_at / 1000, 'unixepoch', 'localtime'))
         WHERE status = 0 AND remind_at IS NOT NULL
           AND date != MAX(?1, strftime('%Y-%m-%d', remind_at / 1000, 'unixepoch', 'localtime'))",
        params![today()],
    )?;
    rollover_tasks(&conn, &today())?;
    Ok(conn)
}

/// 只顺延归属日,保留提醒及触发状态,避免每天重复提醒。
pub fn rollover_tasks(conn: &Connection, date: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE tasks SET date = ?1 WHERE status = 0 AND date < ?1",
        params![date],
    )
}

/// 打开(必要时创建)数据库并跑迁移
pub fn init(app: &AppHandle) -> Result<Db, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("取应用数据目录失败: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建数据目录失败: {e}"))?;
    let path = dir.join("weldd.db");
    let conn = open_conn(&path).map_err(|e| format!("初始化数据库失败: {e}"))?;
    Ok(Db {
        conn: Mutex::new(conn),
        path,
    })
}

/// 今日任务。完成的沉底。
pub fn today_tasks(conn: &Connection, date: &str) -> rusqlite::Result<Vec<Task>> {
    let sql = format!(
        "SELECT {TASK_COLS} FROM tasks WHERE date = ?1 ORDER BY status ASC, sort_order ASC, id ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![date], row_to_task)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn create_task(
    conn: &Connection,
    title: &str,
    remind_at: Option<i64>,
    date: &str,
) -> rusqlite::Result<Task> {
    // list 列走建表默认值 'today'
    conn.execute(
        "INSERT INTO tasks (title, date, remind_at, sort_order, created_at)
         VALUES (?1, ?2, ?3,
                 (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM tasks WHERE date = ?2),
                 ?4)",
        params![title, date, remind_at, now_ms()],
    )?;
    let id = conn.last_insert_rowid();
    let mut stmt = conn.prepare(&format!("SELECT {TASK_COLS} FROM tasks WHERE id = ?1"))?;
    stmt.query_row(params![id], row_to_task)
}

pub fn update_task_title(conn: &Connection, id: i64, title: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE tasks SET title = ?2 WHERE id = ?1",
        params![id, title],
    )
}

pub fn update_task_note(conn: &Connection, id: i64, note: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE tasks SET note = ?2 WHERE id = ?1",
        params![id, note],
    )
}

pub fn set_task_status(conn: &Connection, id: i64, done: bool) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE tasks SET status = ?2, completed_at = ?3 WHERE id = ?1",
        params![id, i64::from(done), if done { Some(now_ms()) } else { None }],
    )
}

/// `remind_at = None` 表示清除提醒(同时清掉已触发标记,允许重新设)。
/// 带提醒时归属日跟提醒日走(date 由命令层按 remind_at 算好传入),
/// 否则提醒响了任务却不在那天的清单里
pub fn set_task_reminder(
    conn: &Connection,
    id: i64,
    remind_at: Option<i64>,
    date: &str,
) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE tasks SET remind_at = ?2,
                remind_fired_at = NULL,
                date = CASE WHEN ?2 IS NULL THEN date ELSE ?3 END
         WHERE id = ?1",
        params![id, remind_at, date],
    )
}

pub fn delete_task(conn: &Connection, id: i64) -> rusqlite::Result<usize> {
    conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])
}

/// 单日历史统计(日历格子)
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DayStat {
    pub date: String,
    pub total: i64,
    pub done: i64,
}

/// 某月每日统计(month 形如 "2026-09")。归属日按任务 date 列算,
/// 迟到补完成也算归属日完成,保证"完成 ≤ 总数"且与未完成口径互补。
/// 没有任务的日子不在结果里,由前端补零。
pub fn month_stats(conn: &Connection, month: &str) -> rusqlite::Result<Vec<DayStat>> {
    let mut stmt = conn.prepare(
        "SELECT date, COUNT(*), COALESCE(SUM(status = 1), 0) FROM tasks
         WHERE date LIKE ?1 || '-%' GROUP BY date ORDER BY date ASC",
    )?;
    let rows = stmt
        .query_map(params![month], |row| {
            Ok(DayStat {
                date: row.get(0)?,
                total: row.get(1)?,
                done: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 某天全部任务(历史明细,只读展示)。完成沉底,与今日视图同序。
pub fn day_tasks(conn: &Connection, date: &str) -> rusqlite::Result<Vec<Task>> {
    let sql = format!(
        "SELECT {TASK_COLS} FROM tasks WHERE date = ?1 ORDER BY status ASC, sort_order ASC, id ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![date], row_to_task)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 全部任务(导出用,含历史)
pub fn all_tasks(conn: &Connection) -> rusqlite::Result<Vec<Task>> {
    let sql = format!("SELECT {TASK_COLS} FROM tasks ORDER BY date ASC, id ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], row_to_task)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 合并导入备份:以 (created_at, title) 判重,库里已有的跳过,
/// 其余保留原字段、按新 id 插入(id 不回填,避免与现存行撞主键)。
/// 整个导入单事务,中途失败整体回滚,防止半截数据污染清单与提醒调度。
pub fn import_tasks(conn: &mut Connection, tasks: &[Task]) -> rusqlite::Result<(usize, usize)> {
    let tx = conn.transaction()?;
    let mut existing: HashSet<(i64, String)> = {
        let mut stmt = tx.prepare("SELECT created_at, title FROM tasks")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut imported = 0;
    let mut skipped = 0;
    {
        let mut ins = tx.prepare(
            "INSERT INTO tasks
               (title, note, status, date, remind_at, remind_fired_at, sort_order, created_at, completed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        for t in tasks {
            let key = (t.created_at, t.title.clone());
            if t.title.trim().is_empty() || existing.contains(&key) {
                skipped += 1;
                continue;
            }
            ins.execute(params![
                t.title,
                t.note,
                t.status,
                t.date,
                t.remind_at,
                t.remind_fired_at,
                t.sort_order,
                t.created_at,
                t.completed_at
            ])?;
            existing.insert(key);
            imported += 1;
        }
    }
    tx.commit()?;
    Ok((imported, skipped))
}

/// 最早的未触发提醒时刻(只看待办任务)
pub fn next_reminder_ms(conn: &Connection) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT MIN(remind_at) FROM tasks
         WHERE status = 0 AND remind_fired_at IS NULL AND remind_at IS NOT NULL",
        [],
        |row| row.get::<_, Option<i64>>(0),
    )
}

/// 已到期但未触发的待办任务
pub fn due_tasks(conn: &Connection, now: i64) -> rusqlite::Result<Vec<Task>> {
    let sql = format!(
        "SELECT {TASK_COLS} FROM tasks
         WHERE status = 0 AND remind_fired_at IS NULL AND remind_at IS NOT NULL AND remind_at <= ?1
         ORDER BY remind_at ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![now], row_to_task)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 全部"已过期未完成"任务(不管是否已触发过提醒)。
/// 到期弹框的数据源:不能按 remind_fired_at IS NULL 过滤,否则窗口加载完
/// 查询时调度器已把触发标记写上,列表永远是空的(与 mark_fired 竞态)
pub fn overdue_tasks(conn: &Connection, now: i64) -> rusqlite::Result<Vec<Task>> {
    let sql = format!(
        "SELECT {TASK_COLS} FROM tasks
         WHERE status = 0 AND remind_at IS NOT NULL AND remind_at <= ?1
         ORDER BY remind_at ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![now], row_to_task)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn mark_fired(conn: &Connection, ids: &[i64], now: i64) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare("UPDATE tasks SET remind_fired_at = ?2 WHERE id = ?1")?;
    for id in ids {
        stmt.execute(params![id, now])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollover_preserves_history_future_tasks_and_reminders() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(MIGRATION).unwrap();
        let old = create_task(&conn, "跨月待办", Some(1234), "2026-10-31").unwrap();
        update_task_note(&conn, old.id, "保留正文").unwrap();
        mark_fired(&conn, &[old.id], 5678).unwrap();
        create_task(&conn, "昨日待办", None, "2026-11-09").unwrap();
        let done = create_task(&conn, "已完成", None, "2026-11-09").unwrap();
        set_task_status(&conn, done.id, true).unwrap();
        create_task(&conn, "明日待办", None, "2026-11-11").unwrap();
        create_task(&conn, "今日待办", None, "2026-11-10").unwrap();

        assert_eq!(rollover_tasks(&conn, "2026-11-10").unwrap(), 2);
        assert_eq!(rollover_tasks(&conn, "2026-11-10").unwrap(), 0);
        let tasks = today_tasks(&conn, "2026-11-10").unwrap();
        assert_eq!(tasks.len(), 3);
        let moved = tasks.iter().find(|t| t.id == old.id).unwrap();
        assert_eq!(moved.note, "保留正文");
        assert_eq!(moved.remind_at, Some(1234));
        assert_eq!(moved.remind_fired_at, Some(5678));
        assert_eq!(moved.created_at, old.created_at);
        assert!(due_tasks(&conn, 9999).unwrap().is_empty());
        assert_eq!(day_tasks(&conn, "2026-11-09").unwrap()[0].id, done.id);
        assert_eq!(day_tasks(&conn, "2026-11-11").unwrap().len(), 1);
        assert_eq!(rollover_tasks(&conn, "2026-11-12").unwrap(), 4);
        assert_eq!(today_tasks(&conn, "2026-11-12").unwrap().len(), 4);
    }

    #[test]
    fn reopening_database_keeps_carried_tasks_today() {
        let path = std::env::temp_dir().join(format!("weldd-rollover-{}-{}.db", std::process::id(), now_ms()));
        {
            let conn = open_conn(&path).unwrap();
            create_task(&conn, "旧提醒", Some(1234), "1970-01-01").unwrap();
            create_task(&conn, "无提醒", None, "2020-01-01").unwrap();
        }
        for _ in 0..2 {
            let conn = open_conn(&path).unwrap();
            assert_eq!(today_tasks(&conn, &today()).unwrap().len(), 2);
            assert_eq!(rollover_tasks(&conn, &today()).unwrap(), 0);
        }
        std::fs::remove_file(path).unwrap();
    }
}
