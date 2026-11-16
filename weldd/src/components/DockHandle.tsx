import { api } from "../lib/tauri";
import { selectToday, useTasks } from "../stores/tasks";

interface DockHandleProps {
  side: "left" | "right" | "top";
}

/** 边缘停靠把手:贴屏幕左右/顶部边缘的半透明窄条,点击滑出为胶囊 */
export default function DockHandle({ side }: DockHandleProps) {
  // 停靠时岛身不可见,把手上的红色徽章/红点承担待办与过期提醒,不让任务"沉没"
  const tasks = useTasks((s) => s.tasks);
  const { open, overdue } = selectToday(tasks);
  const count = open.length;
  const hasOverdue = overdue.length > 0;

  // 只点击滑出,不做悬停自动展开:岛贴着屏幕顶缘,顶部是鼠标高频路过区,
  // 自动滑出会"自己弹出来"打断操作(用户实测反馈)
  const slideOut = () => {
    api.setIslandMode("collapsed").catch(console.error);
  };

  return (
    <div className={`dock-root dock-${side}`}>
      <button
        type="button"
        className={`dock-handle ${hasOverdue ? "has-overdue" : ""} ${count > 0 ? "has-todos" : ""}`}
        title={
          hasOverdue
            ? `⚠ 有过期任务,共 ${count} 项待办,点击滑出处理`
            : count > 0
              ? `${count} 项待办,点击滑出`
              : "点击滑出灵动岛"
        }
        onClick={slideOut}
      >
        {side === "top" ? (
          <>
            <span className="dock-label">
              <svg width="11" height="11" viewBox="0 0 12 12" fill="none" aria-hidden="true">
                <path
                  d="M1.5 3 2.5 4 4 2M6 3h4.5M1.5 8 2.5 9 4 7M6 8h4.5"
                  stroke="currentColor"
                  strokeWidth="1.2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
              todo
            </span>
            {/* 顶部贴边时右侧的红色待办数小圆,数量变化时靠 key 重放 pop */}
            {count > 0 && (
              <span
                className={`dock-badge ${hasOverdue ? "badge-pulse" : ""}`}
                key={count}
                aria-label={`今日待办 ${count} 项`}
              >
                {count > 99 ? "99+" : count}
              </span>
            )}
          </>
        ) : (
          <svg width="7" height="26" viewBox="0 0 7 26" fill="none" aria-hidden="true">
            <path
              d={side === "left" ? "M5 4 2 13l3 9" : "M2 4l3 9-3 9"}
              stroke="currentColor"
              strokeWidth="1.6"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        )}
        {hasOverdue && side !== "top" && <span className="dock-alert-dot" aria-hidden="true" />}
      </button>
    </div>
  );
}
