import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, restoreGlassOpacity, type IslandMode, type ReminderDue } from "./lib/tauri";
import { useTasks } from "./stores/tasks";
import { useUi } from "./stores/ui";
import Pill from "./components/Pill";
import Panel from "./components/Panel";
import DockHandle from "./components/DockHandle";

/** hover 进入展开的防抖(毫秒) */
const EXPAND_DELAY = 150;
/** 鼠标离开面板回收的延迟(毫秒):用户要求回收干脆,只留毛刺宽限 */
const COLLAPSE_DELAY = 100;
/** 提醒到达后岛动效的持续时间(毫秒) */
const ALERT_DURATION = 6000;

export default function App() {
  const mode = useUi((s) => s.mode);
  const setMode = useUi((s) => s.setMode);
  const bumpQuickAdd = useUi((s) => s.bumpQuickAdd);
  const setSettingsOpen = useUi((s) => s.setSettingsOpen);
  const fetchTasks = useTasks((s) => s.fetch);

  const [modeReady, setModeReady] = useState(false);
  const [alert, setAlert] = useState<ReminderDue | null>(null);
  const expandTimer = useRef<number | undefined>(undefined);
  const collapseTimer = useRef<number | undefined>(undefined);
  const alertTimer = useRef<number | undefined>(undefined);
  /** 鼠标是否在窗口内:输入框失焦时若鼠标已离开则回收面板 */
  const pointerInside = useRef(false);

  useEffect(() => {
    let disposed = false;
    let revision = 0;
    let unsubscribe: (() => void) | undefined;
    // 先订阅再取 Rust 快照,既补启动状态,也避免旧快照覆盖新事件。
    void listen<IslandMode>("island://mode", (event) => {
      if (disposed) return;
      revision += 1;
      window.clearTimeout(expandTimer.current);
      window.clearTimeout(collapseTimer.current);
      setMode(event.payload);
      setModeReady(true);
    }).then(async (unlisten) => {
      unsubscribe = unlisten;
      if (disposed) { unlisten(); return; }
      const before = revision;
      const current = await api.getIslandMode();
      if (!disposed && revision === before) {
        setMode(current);
        setModeReady(true);
      }
    }).catch(console.error);
    return () => { disposed = true; unsubscribe?.(); };
  }, [setMode]);

  useEffect(() => {
    const unsubscribers = [
      listen("island://quick-add", () => bumpQuickAdd()),
      listen("island://open-settings", () => setSettingsOpen(true)),
      listen("tasks://changed", () => void fetchTasks()),
      listen<ReminderDue>("reminder://due", (event) => {
        setAlert(event.payload);
        window.clearTimeout(alertTimer.current);
        alertTimer.current = window.setTimeout(() => setAlert(null), ALERT_DURATION);
      }),
    ];
    void fetchTasks();
    void restoreGlassOpacity();
    return () => {
      window.clearTimeout(expandTimer.current);
      window.clearTimeout(collapseTimer.current);
      window.clearTimeout(alertTimer.current);
      unsubscribers.forEach((p) => p.then((un) => un()));
    };
  }, [fetchTasks, setMode, bumpQuickAdd, setSettingsOpen]);

  // 跨零点后"今日"清单自动刷新(每分钟对比一次本地日期)
  useEffect(() => {
    let lastDay = new Date().toDateString();
    const timer = window.setInterval(() => {
      const today = new Date().toDateString();
      if (today !== lastDay) {
        lastDay = today;
        void fetchTasks();
      }
    }, 60_000);
    return () => window.clearInterval(timer);
  }, [fetchTasks]);

  const expand = useCallback(() => {
    window.clearTimeout(collapseTimer.current);
    window.clearTimeout(expandTimer.current);
    if (useUi.getState().mode !== "collapsed") return;
    expandTimer.current = window.setTimeout(() => {
      if (!pointerInside.current) return;
      api.setIslandMode("expanded").catch(console.error);
    }, EXPAND_DELAY);
  }, []);

  const collapse = useCallback(() => {
    window.clearTimeout(expandTimer.current);
    // 编辑和配置期间保持展开,避免草稿随自动收起丢失。
    if (useUi.getState().settingsOpen || document.activeElement?.matches("input, textarea, select, [contenteditable=true]")) return;
    window.clearTimeout(collapseTimer.current);
    collapseTimer.current = window.setTimeout(() => {
      if (pointerInside.current || useUi.getState().settingsOpen || document.activeElement?.matches("input, textarea, select, [contenteditable=true]")) return;
      api.setIslandMode("collapsed").catch(console.error);
    }, COLLAPSE_DELAY);
  }, []);

  const toggle = useCallback(() => {
    window.clearTimeout(expandTimer.current);
    window.clearTimeout(collapseTimer.current);
    const next = mode === "collapsed" ? "expanded" : "collapsed";
    api.setIslandMode(next).catch(console.error);
  }, [mode]);

  if (!modeReady) return null;

  if (mode === "docked-left" || mode === "docked-right" || mode === "docked-top") {
    return (
      <DockHandle
        side={mode === "docked-left" ? "left" : mode === "docked-right" ? "right" : "top"}
      />
    );
  }

  return (
    <div
      className="island-root"
      onMouseEnter={() => {
        pointerInside.current = true;
        window.clearTimeout(collapseTimer.current);
      }}
      onMouseLeave={() => {
        pointerInside.current = false;
        window.clearTimeout(expandTimer.current);
        if (mode === "expanded") collapse();
      }}
    >
      <Pill
        expanded={mode === "expanded"}
        alert={alert}
        onHover={expand}
        onToggle={toggle}
        onMaybeCollapse={collapse}
      />
      <div className="panel-viewport" inert={mode !== "expanded"}>
        <Panel onMaybeCollapse={collapse} pointerInside={pointerInside} />
      </div>
    </div>
  );
}
