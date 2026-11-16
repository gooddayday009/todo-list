import { fmtHHMM } from "../lib/tauri";
import { selectToday, useTasks } from "../stores/tasks";
import { useUi } from "../stores/ui";

/** 面板顶部概览卡:今日待办大数字 + 已完成 + 下一提醒 */
export default function OverviewCard() {
  const tasks = useTasks((s) => s.tasks);
  const setHistoryOpen = useUi((s) => s.setHistoryOpen);
  const setSettingsOpen = useUi((s) => s.setSettingsOpen);
  const { open, done, nextRemind, overdue } = selectToday(tasks);

  const total = open.length + done.length;
  const progress = total ? Math.round(done.length / total * 100) : 0;

  return (
    <div className={`overview-card ${overdue.length > 0 ? "overview-overdue" : ""}`}>
      <div className="overview-heading">
        <div>
          <span className="overview-eyebrow">今日概览</span>
          <div className="overview-count">
            <span className="overview-num">{open.length}</span>
            <span className="overview-unit">项待办</span>
            {overdue.length > 0 && <span className="overview-value red">· {overdue.length} 项已过期</span>}
          </div>
        </div>
        <div className="overview-actions">
          <button
            type="button"
            className="overview-btn"
            title="历史报表"
            onClick={() => setHistoryOpen(true)}
          >
            <svg width="13" height="13" viewBox="0 0 14 14" fill="none" aria-hidden="true">
              <path
                d="M3 12V7.5M7 12V2.5M11 12V5"
                stroke="currentColor"
                strokeWidth="1.6"
                strokeLinecap="round"
              />
            </svg>
          </button>
          <button
            type="button"
            className="overview-gear"
            title="设置"
            onClick={() => setSettingsOpen(true)}
          >
            <svg width="13" height="13" viewBox="0 0 14 14" fill="none" aria-hidden="true">
              <circle cx="7" cy="7" r="2.1" stroke="currentColor" strokeWidth="1.3" />
              <path
                d="M7 1.2v1.6M7 11.2v1.6M1.2 7h1.6M11.2 7h1.6M2.9 2.9l1.1 1.1M10 10l1.1 1.1M11.1 2.9 10 4M4 10l-1.1 1.1"
                stroke="currentColor"
                strokeWidth="1.3"
                strokeLinecap="round"
              />
            </svg>
          </button>
        </div>
      </div>
      <div className="overview-meta">
        <span>已完成 <strong>{done.length}</strong> / {total}</span>
        <span>下一提醒 <strong className="cyan">{nextRemind !== undefined ? fmtHHMM(nextRemind) : "暂无"}</strong></span>
      </div>
      <div className="overview-progress" role="progressbar" aria-label="今日完成进度" aria-valuenow={progress} aria-valuemin={0} aria-valuemax={100}>
        <span style={{ width: `${progress}%` }} />
      </div>
    </div>
  );
}
