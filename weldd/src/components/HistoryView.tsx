import { useEffect, useMemo, useState } from "react";
import { api, type DayStat, type Task } from "../lib/tauri";
import { useUi } from "../stores/ui";

/** 本地时区 "YYYY-MM-DD"(与 Rust 侧 date 列同一格式) */
function ymd(d: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

const WEEKDAYS = ["一", "二", "三", "四", "五", "六", "日"];

/** 历史报表:月历总览 + 当日明细。任务按归属日(date)入史,完成沉底;只读,不改动历史 */
export default function HistoryView() {
  const setHistoryOpen = useUi((s) => s.setHistoryOpen);

  const now = new Date();
  const [year, setYear] = useState(now.getFullYear());
  const [month, setMonth] = useState(now.getMonth()); // 0 起算
  const [selected, setSelected] = useState(() => ymd(now));
  const [stats, setStats] = useState<DayStat[]>([]);
  const [dayTasks, setDayTasks] = useState<Task[]>([]);

  const monthKey = `${year}-${String(month + 1).padStart(2, "0")}`;

  useEffect(() => {
    api.listMonthStats(monthKey).then(setStats).catch(console.error);
  }, [monthKey]);

  useEffect(() => {
    api.listDayTasks(selected).then(setDayTasks).catch(console.error);
  }, [selected]);

  const statMap = useMemo(() => new Map(stats.map((s) => [s.date, s])), [stats]);

  // 月历格子:周一开始,前置空位补齐;null 表示空白格
  const cells = useMemo(() => {
    const lead = (new Date(year, month, 1).getDay() + 6) % 7;
    const days = new Date(year, month + 1, 0).getDate();
    return [...Array<null>(lead).fill(null), ...Array.from({ length: days }, (_, i) => i + 1)];
  }, [year, month]);

  const sumDone = stats.reduce((a, s) => a + s.done, 0);
  const sumTotal = stats.reduce((a, s) => a + s.total, 0);
  const doneCount = dayTasks.filter((t) => t.status === 1).length;

  const shiftMonth = (delta: number) => {
    const m = new Date(year, month + delta, 1);
    setYear(m.getFullYear());
    setMonth(m.getMonth());
  };

  const todayKey = ymd(now);
  const [sy, sm, sd] = selected.split("-").map(Number);
  const selectedDate = new Date(sy, sm - 1, sd);
  const selectedLabel = `${sm}月${sd}日 周${WEEKDAYS[(selectedDate.getDay() + 6) % 7]}`;

  return (
    <div className="history">
      <header className="settings-header">
        <button
          type="button"
          className="settings-back"
          onClick={() => setHistoryOpen(false)}
          title="返回"
        >
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden="true">
            <path
              d="M7.5 2 4 6l3.5 4"
              stroke="currentColor"
              strokeWidth="1.6"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </button>
        <span className="settings-title">历史报表</span>
      </header>

      <nav className="hist-nav">
        <button type="button" className="hist-nav-btn" onClick={() => shiftMonth(-1)} title="上个月">
          <svg width="10" height="10" viewBox="0 0 12 12" fill="none" aria-hidden="true">
            <path
              d="M7.5 2 4 6l3.5 4"
              stroke="currentColor"
              strokeWidth="1.6"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </button>
        <span className="hist-month">
          {year}年{month + 1}月
        </span>
        <button type="button" className="hist-nav-btn" onClick={() => shiftMonth(1)} title="下个月">
          <svg width="10" height="10" viewBox="0 0 12 12" fill="none" aria-hidden="true">
            <path
              d="M4.5 2 8 6l-3.5 4"
              stroke="currentColor"
              strokeWidth="1.6"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </button>
        <span className="hist-summary">
          完成 <b>{sumDone}</b> · 未完成 <b className="red">{sumTotal - sumDone}</b>
        </span>
      </nav>

      <div className="hist-week" aria-hidden="true">
        {WEEKDAYS.map((w) => (
          <span key={w}>{w}</span>
        ))}
      </div>

      <div className="hist-grid">
        {cells.map((day, i) => {
          if (day === null) return <span key={`e${i}`} className="hist-cell empty" />;
          const key = `${monthKey}-${String(day).padStart(2, "0")}`;
          const stat = statMap.get(key);
          const cls = [
            "hist-cell",
            stat && stat.done === stat.total ? "all-done" : "",
            stat && stat.done < stat.total ? "undone" : "",
            key === todayKey ? "today" : "",
            key === selected ? "selected" : "",
          ]
            .filter(Boolean)
            .join(" ");
          return (
            <button
              type="button"
              key={key}
              className={cls}
              onClick={() => setSelected(key)}
              title={stat ? `完成 ${stat.done} / ${stat.total}` : undefined}
            >
              <span className="hist-day">{day}</span>
              {stat && (
                <span className="hist-bar">
                  <i
                    className={stat.done === 0 ? "zero" : ""}
                    style={{ width: `${(stat.done / stat.total) * 100}%` }}
                  />
                </span>
              )}
            </button>
          );
        })}
      </div>

      <section className="hist-detail">
        <header className="hist-detail-head">
          <span className="hist-detail-date">{selectedLabel}</span>
          {dayTasks.length > 0 && (
            <span className="hist-detail-count">
              完成 {doneCount} · 未完成 {dayTasks.length - doneCount}
            </span>
          )}
        </header>
        {dayTasks.length === 0 ? (
          <p className="hist-empty">这一天没有任务</p>
        ) : (
          dayTasks.map((t) => {
            const done = t.status === 1;
            return (
              <div key={t.id} className={`hist-task ${done ? "done" : ""}`}>
                <span className={`hist-check ${done ? "checked" : ""}`} aria-hidden="true">
                  {done && (
                    <svg width="8" height="8" viewBox="0 0 10 10" fill="none">
                      <path
                        d="M1.5 5.2 4 7.7 8.5 2.6"
                        stroke="#10241a"
                        strokeWidth="1.8"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                      />
                    </svg>
                  )}
                </span>
                <span className="hist-title">{t.title}</span>
              </div>
            );
          })
        )}
      </section>
    </div>
  );
}
