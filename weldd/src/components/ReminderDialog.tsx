import { useCallback, useEffect, useState } from "react";
import { api, type Task } from "../lib/tauri";

const missed = new URLSearchParams(window.location.search).get("kind") === "missed";

/**
 * 到期提醒弹框:Rust 调度器到点时创建的居中小窗(reminder.rs)。
 * 数据统一从 list_due_tasks 拉"全部过期未完成"任务(不按 remind_fired_at
 * 过滤,与 mark_fired 有竞态,见 db::overdue_tasks);列表只显示任务标题 +
 * "到期"状态字(用户拍板:非按钮,不在弹框里操作,处理去面板);
 * "知道了"/Esc 关窗,任务保持"过期未处理"的岛提示,处理完才消失。
 */
export default function ReminderDialog() {
  const [due, setDue] = useState<Task[]>([]);

  const refresh = useCallback(() => {
    api.listDueTasks().then(setDue).catch(console.error);
  }, []);

  const close = useCallback(() => {
    api.closeReminderDialog().catch(console.error);
  }, []);

  useEffect(() => {
    refresh();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [refresh, close]);

  return (
    <div className="reminder-overlay">
      <div className="reminder-card">
        <div className="reminder-head">
          <span className="reminder-icon">⏰</span>
          <div>
            <h1 className="reminder-title">
              {missed ? "有任务提醒已过期" : "任务到期"}
            </h1>
            <p className="reminder-sub">
              {due.length > 1 ? `共 ${due.length} 项待处理` : "请及时处理"}
            </p>
          </div>
        </div>
        <ul className="reminder-list">
          {due.map((t) => (
            <li key={t.id} className="reminder-item">
              <span className="reminder-item-title">{t.title}</span>
              <span className="reminder-due-tag">到期</span>
            </li>
          ))}
        </ul>
        <button type="button" className="reminder-ack" onClick={close}>
          知道了
        </button>
      </div>
    </div>
  );
}
