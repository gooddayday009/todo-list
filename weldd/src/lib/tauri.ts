/** Tauri bridge:类型化 invoke 封装 + 时间/快捷语法工具 */

import { invoke } from "@tauri-apps/api/core";

export type IslandMode =
  | "collapsed"
  | "expanded"
  | "docked-left"
  | "docked-right"
  | "docked-top";

export interface Task {
  id: number;
  title: string;
  note: string;
  status: number; // 0 待办 1 已完成
  date: string;
  remindAt: number | null;
  remindFiredAt: number | null;
  sortOrder: number;
  createdAt: number;
  completedAt: number | null;
}

export interface ReminderDue {
  kind: "due" | "missed";
  /** 触发批次里最早到期任务的标题,多条时配合 count 显示 */
  title: string;
  count?: number;
}

/** 单日历史统计(日历格子);date 形如 "2026-09-08" */
export interface DayStat {
  date: string;
  total: number;
  done: number;
}

/** 导出结果;用户取消对话框时命令返回 null */
export interface ExportResult {
  count: number;
  path: string;
}

/** 导入结果(合并导入,重复任务跳过) */
export interface ImportResult {
  imported: number;
  skipped: number;
}

export interface WebhookChannel {
  platform: "wecom" | "feishu" | "dingtalk";
  enabled: boolean;
  url: string;
  secret: string;
}

export const api = {
  getIslandMode: () => invoke<IslandMode>("get_island_mode"),
  getWebhookConfig: () => invoke<WebhookChannel[]>("get_webhook_config"),
  saveWebhookConfig: (channels: WebhookChannel[]) => invoke<void>("save_webhook_config", { channels }),
  testWebhook: (channel: WebhookChannel) => invoke<void>("test_webhook", { channel }),
  setIslandMode: (mode: IslandMode) => invoke<void>("set_island_mode", { mode }),
  dragIsland: () => invoke<void>("drag_island"),
  hideIsland: () => invoke<void>("hide_island"),
  /** 打开设置页:与托盘"设置"同一路径 */
  openSettings: () => invoke<void>("open_island_settings"),
  listTodayTasks: () => invoke<Task[]>("list_today_tasks"),
  listMonthStats: (month: string) => invoke<DayStat[]>("list_month_stats", { month }),
  listDayTasks: (date: string) => invoke<Task[]>("list_day_tasks", { date }),
  /** 到期未处理任务(全屏提醒对话框数据源) */
  listDueTasks: () => invoke<Task[]>("list_due_tasks"),
  /** 关闭到期全屏对话框窗口 */
  closeReminderDialog: () => invoke<void>("close_reminder_dialog"),
  createTask: (title: string, remindAt: number | null) =>
    invoke<Task>("create_task", { title, remindAt }),
  updateTaskTitle: (id: number, title: string) =>
    invoke<void>("update_task_title", { id, title }),
  updateTaskNote: (id: number, note: string) =>
    invoke<void>("update_task_note", { id, note }),
  setTaskStatus: (id: number, done: boolean) =>
    invoke<void>("set_task_status", { id, done }),
  setTaskReminder: (id: number, remindAt: number | null) =>
    invoke<void>("set_task_reminder", { id, remindAt }),
  deleteTask: (id: number) => invoke<void>("delete_task", { id }),
  getAutostart: () => invoke<boolean>("get_autostart"),
  setAutostart: (enabled: boolean) => invoke<void>("set_autostart", { enabled }),
  resetIslandPosition: () => invoke<void>("reset_island_position"),
  getUiSetting: (key: string) => invoke<number | boolean | null>("get_ui_setting", { key }),
  setUiSetting: (key: string, value: number | boolean) =>
    invoke<void>("set_ui_setting", { key, value }),
  exportTasks: () => invoke<ExportResult | null>("export_tasks"),
  importTasks: () => invoke<ImportResult | null>("import_tasks"),
};

// ---------- 透明度 ----------

// 毛玻璃底色:深色高不透明(灵动岛式黑玻璃),保证任意壁纸上文字可读。
// WebView2 里 CSS backdrop-filter 模糊不了桌面,所以"玻璃感"主要靠
// 这层底色 + 描边 + 投影,透明度不能给得太低。
const GLASS_BG_RGB = "17, 19, 23";
const GLASS_BASE_BG_ALPHA = 0.9;
const GLASS_BORDER_RGB = "255, 255, 255";
const GLASS_BASE_BORDER_ALPHA = 0.2;
const GLASS_HOVER_RGB = "41, 45, 52";
const GLASS_BASE_HOVER_ALPHA = 0.94;

export const DEFAULT_OPACITY = 100;
/** 透明度下限:再低文字就看不清了 */
export const MIN_OPACITY = 70;

/** 按百分比(55–100)缩放毛玻璃透明度,实时生效 */
export function applyGlassOpacity(percent: number) {
  const f = Math.min(100, Math.max(MIN_OPACITY, percent)) / 100;
  const root = document.documentElement.style;
  root.setProperty("--glass-bg", `rgba(${GLASS_BG_RGB}, ${(GLASS_BASE_BG_ALPHA * f).toFixed(3)})`);
  root.setProperty(
    "--glass-border",
    `rgba(${GLASS_BORDER_RGB}, ${(GLASS_BASE_BORDER_ALPHA * f).toFixed(3)})`,
  );
  root.setProperty(
    "--glass-hover",
    `rgba(${GLASS_HOVER_RGB}, ${(GLASS_BASE_HOVER_ALPHA * f).toFixed(3)})`,
  );
}

/** 启动时恢复已保存的透明度 */
export async function restoreGlassOpacity() {
  try {
    const saved = await api.getUiSetting("ui_opacity");
    applyGlassOpacity(typeof saved === "number" ? saved : DEFAULT_OPACITY);
  } catch {
    applyGlassOpacity(DEFAULT_OPACITY);
  }
}

// ---------- 时间工具 ----------

export function fmtHHMM(ts: number): string {
  const d = new Date(ts);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** "14:30" / "明天 14:30" / "9/10 14:30" 之类的提醒时刻标签 */
export function fmtRemindLabel(ts: number): string {
  const d = new Date(ts);
  const now = new Date();
  const dayStart = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const dayDiff = Math.round((dayStart(d) - dayStart(now)) / 86_400_000);
  const hhmm = fmtHHMM(ts);
  if (dayDiff === 0) return hhmm;
  if (dayDiff === 1) return `明天 ${hhmm}`;
  return `${d.getMonth() + 1}/${d.getDate()} ${hhmm}`;
}

/**
 * 解析输入框快捷语法:标题里的 `14:30` / `明天 14:30` 自动转为提醒时刻。
 * - 今天已过的时刻自动顺延到明天
 * - 返回 null 表示没有识别到时间语法
 */
export function parseQuickSyntax(raw: string): {
  title: string;
  remindAt: number | null;
} {
  const m = raw.match(/(明天|后天)?\s*([0-9]{1,2}):([0-9]{2})/);
  if (!m) {
    return { title: raw.trim(), remindAt: null };
  }
  const hours = parseInt(m[2], 10);
  const minutes = parseInt(m[3], 10);
  if (hours > 23 || minutes > 59) {
    return { title: raw.trim(), remindAt: null };
  }
  const title = raw.replace(m[0], "").replace(/\s{2,}/g, " ").trim();
  const dayOffset = m[1] === "明天" ? 1 : m[1] === "后天" ? 2 : 0;
  const now = new Date();
  const at = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate() + dayOffset,
    hours,
    minutes,
    0,
    0,
  );
  if (dayOffset === 0 && at.getTime() <= now.getTime()) {
    at.setDate(at.getDate() + 1);
  }
  return {
    title: title || `${m[1] ?? ""}${m[2]}:${m[3]} 的提醒`.trim(),
    remindAt: at.getTime(),
  };
}
