import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState, type MouseEvent } from "react";
import { api, fmtRemindLabel, type ReminderDue } from "../lib/tauri";
import { selectToday, useTasks } from "../stores/tasks";

interface PillProps {
  expanded: boolean;
  alert: ReminderDue | null;
  onHover: () => void;
  onToggle: () => void;
  onMaybeCollapse: () => void;
}

/** 折叠胶囊:绿点图标 + 今日概要 + 圆形展开钮(设计稿视觉) */
export default function Pill({ expanded, alert, onHover, onToggle, onMaybeCollapse }: PillProps) {
  const tasks = useTasks((s) => s.tasks);
  const { open, next, overdue } = selectToday(tasks);
  const [menuOpen, setMenuOpen] = useState(false);
  const menuTimer = useRef<number | undefined>(undefined);

  useEffect(() => () => window.clearTimeout(menuTimer.current), []);

  const handleDrag = (e: MouseEvent<HTMLElement>) => {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button, .pill-menu")) return;
    invoke("drag_island").catch(console.error);
  };

  const openMenu = (e: MouseEvent<HTMLElement>) => {
    e.preventDefault();
    setMenuOpen(true);
  };

  const menuAction = (action: () => void) => {
    setMenuOpen(false);
    action();
  };

  // 文案优先级:提醒到达动效(6s) > 持久过期提示 > 常规概要
  const sub = alert
    ? `⏰ ${alert.title}${alert.count && alert.count > 1 ? ` 等 ${alert.count} 项` : ""}`
    : overdue.length > 0
      ? overdue.length === 1
        ? `「${overdue[0].title}」已过期,请及时处理`
        : `「${overdue[0].title}」等 ${overdue.length} 项已过期,请及时处理`
      : open.length > 0
        ? `今日 ${open.length} 项 · 下一项:${next?.title ?? ""}`
        : "今日无任务 · 悬停展开添加";

  const alarming = !!alert || overdue.length > 0;

  return (
    <header
      className={`pill ${alert ? "pill-alert" : ""} ${overdue.length > 0 ? "pill-overdue" : ""}`}
      onMouseDown={handleDrag}
      onMouseEnter={onHover}
      onContextMenu={openMenu}
      title={
        overdue.length > 0
          ? "有过期任务,点击展开处理;按住拖动调整位置"
          : "按住拖动调整位置,右键更多选项"
      }
    >
      <div className={`status-dot ${alarming ? "dot-alert" : ""}`} aria-hidden="true" />
      <div className="pill-text">
        <span className="app-name">待办</span>
        <span className="pill-divider" aria-hidden="true" />
        <span className="pill-sub">{sub}</span>
        {next?.remindAt && !alarming && (
          <span className="pill-count">{fmtRemindLabel(next.remindAt)}</span>
        )}
      </div>

      {menuOpen && (
        <div className="pill-menu" onMouseLeave={() => setMenuOpen(false)}>
          <button type="button" onClick={() => menuAction(() => void api.setIslandMode("docked-left"))}>
            停靠到左缘
          </button>
          <button type="button" onClick={() => menuAction(() => void api.setIslandMode("docked-right"))}>
            停靠到右缘
          </button>
          <button type="button" onClick={() => menuAction(() => void api.setIslandMode("docked-top"))}>
            停靠到上方
          </button>
          <button
            type="button"
            onClick={() =>
              menuAction(() => {
                onMaybeCollapse();
                void api.setIslandMode("expanded");
              })
            }
          >
            展开面板
          </button>
          <button
            type="button"
            onClick={() => menuAction(() => void api.openSettings())}
          >
            设置
          </button>
          <button
            type="button"
            onClick={() => menuAction(() => void api.hideIsland())}
            className="danger"
          >
            隐藏(Alt+Q 恢复)
          </button>
        </div>
      )}

      <button
        type="button"
        className="pill-btn"
        onClick={onToggle}
        title={expanded ? "收起" : "展开"}
      >
        {expanded ? (
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden="true">
            <path
              d="M2.5 7.5 6 4l3.5 3.5"
              stroke="currentColor"
              strokeWidth="1.6"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        ) : (
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden="true">
            <path
              d="M2.5 4.5 6 8l3.5-3.5"
              stroke="currentColor"
              strokeWidth="1.6"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        )}
      </button>
    </header>
  );
}
