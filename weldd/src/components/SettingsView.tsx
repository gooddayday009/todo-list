import { useEffect, useRef, useState } from "react";
import { api, applyGlassOpacity, DEFAULT_OPACITY, MIN_OPACITY } from "../lib/tauri";
import WebhookSettings from "./WebhookSettings";
import { useUi } from "../stores/ui";

/** 设置页:透明度 / 开机自启 / 岛位置重置 / 快捷键说明 / 关于 */
export default function SettingsView() {
  const setSettingsOpen = useUi((s) => s.setSettingsOpen);
  const [autostart, setAutostart] = useState(false);
  const [autostartBusy, setAutostartBusy] = useState(false);
  const [resetDone, setResetDone] = useState(false);
  const [opacity, setOpacity] = useState(DEFAULT_OPACITY);
  /** 拖动松手靠近屏幕顶部时自动收缩为顶部把手(与 Rust 侧吸附共用) */
  const [topDock, setTopDock] = useState(true);
  const persistTimer = useRef<number | undefined>(undefined);
  /** 导出/导入结果反馈(6s 自动消失,悬停可看完整路径) */
  const [dataMsg, setDataMsg] = useState("");
  const [dataTitle, setDataTitle] = useState("");
  const [dataBusy, setDataBusy] = useState(false);
  const dataTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    api.getAutostart().then(setAutostart).catch(console.error);
    api
      .getUiSetting("ui_opacity")
      .then((saved) => {
        // 旧档可能存过低于下限的值(20),钳到当前下限避免一启动就看不清
        if (typeof saved === "number") {
          setOpacity(Math.min(100, Math.max(MIN_OPACITY, saved)));
        }
      })
      .catch(console.error);
    api
      .getUiSetting("top_dock")
      .then((saved) => {
        if (typeof saved === "boolean") setTopDock(saved);
      })
      .catch(console.error);
  }, []);

  useEffect(
    () => () => {
      window.clearTimeout(persistTimer.current);
      window.clearTimeout(dataTimer.current);
    },
    [],
  );

  const toggleAutostart = async (checked: boolean) => {
    setAutostartBusy(true);
    try {
      await api.setAutostart(checked);
      setAutostart(checked);
    } catch (e) {
      console.error(e);
    } finally {
      setAutostartBusy(false);
    }
  };

  /** 滑杆拖动实时改毛玻璃,停顿 400ms 后才落盘 */
  const changeOpacity = (value: number) => {
    setOpacity(value);
    applyGlassOpacity(value);
    window.clearTimeout(persistTimer.current);
    persistTimer.current = window.setTimeout(() => {
      api.setUiSetting("ui_opacity", value).catch(console.error);
    }, 400);
  };

  const resetPosition = async () => {
    await api.resetIslandPosition().catch(console.error);
    setResetDone(true);
    window.setTimeout(() => setResetDone(false), 1500);
  };

  const toggleTopDock = (checked: boolean) => {
    setTopDock(checked);
    api.setUiSetting("top_dock", checked).catch(console.error);
  };

  const showDataMsg = (msg: string, title = "") => {
    setDataMsg(msg);
    setDataTitle(title);
    window.clearTimeout(dataTimer.current);
    dataTimer.current = window.setTimeout(() => setDataMsg(""), 6000);
  };

  const runExport = async () => {
    setDataBusy(true);
    try {
      const r = await api.exportTasks();
      if (r) showDataMsg(`已导出 ${r.count} 条 → ${r.path}`, r.path);
    } catch (e) {
      showDataMsg(`导出失败:${e}`);
      console.error(e);
    } finally {
      setDataBusy(false);
    }
  };

  const runImport = async () => {
    setDataBusy(true);
    try {
      const r = await api.importTasks();
      if (!r) return;
      if (r.imported > 0) {
        showDataMsg(
          `导入完成:新增 ${r.imported} 条${r.skipped > 0 ? `,跳过 ${r.skipped} 条重复` : ""}`,
        );
      } else {
        showDataMsg(r.skipped > 0 ? "没有新任务,备份里的任务都已存在" : "备份里没有任务");
      }
    } catch (e) {
      showDataMsg(`导入失败:${e}`);
      console.error(e);
    } finally {
      setDataBusy(false);
    }
  };

  return (
    <div className="settings">
      <header className="settings-header">
        <button
          type="button"
          className="settings-back"
          onClick={() => setSettingsOpen(false)}
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
        <span className="settings-title">设置</span>
      </header>

      <section className="settings-group">
        <div className="settings-row settings-row--column">
          <div className="settings-row-head">
            <span className="settings-label">透明度</span>
            <span className="settings-hint">{opacity}%</span>
          </div>
          <input
            type="range"
            className="settings-range"
            min={MIN_OPACITY}
            max={100}
            step={1}
            value={opacity}
            onChange={(e) => changeOpacity(Number(e.target.value))}
          />
        </div>

        <label className="settings-row">
          <span className="settings-label">开机自启</span>
          <input
            type="checkbox"
            className="settings-toggle"
            checked={autostart}
            disabled={autostartBusy}
            onChange={(e) => void toggleAutostart(e.target.checked)}
          />
        </label>

        <label
          className="settings-row"
          title="拖动岛松手时,离屏幕顶部过近会自动收缩为顶部把手"
        >
          <span className="settings-label">靠近顶部自动收边</span>
          <input
            type="checkbox"
            className="settings-toggle"
            checked={topDock}
            onChange={(e) => toggleTopDock(e.target.checked)}
          />
        </label>

        <div className="settings-row">
          <span className="settings-label">岛位置</span>
          <button type="button" className="settings-btn" onClick={() => void resetPosition()}>
            {resetDone ? "已重置 ✓" : "重置到顶部居中"}
          </button>
        </div>
      </section>

      <section className="settings-group">
        <div
          className="settings-row"
          title="导出全部任务(含历史)为 JSON;导入时已存在的任务自动跳过"
        >
          <span className="settings-label">数据备份</span>
          <div className="settings-data-btns">
            <button
              type="button"
              className="settings-btn"
              disabled={dataBusy}
              onClick={() => void runExport()}
            >
              导出…
            </button>
            <button
              type="button"
              className="settings-btn"
              disabled={dataBusy}
              onClick={() => void runImport()}
            >
              导入…
            </button>
          </div>
        </div>
        {dataMsg && (
          <div className="settings-row">
            <span className="settings-msg" title={dataTitle}>
              {dataMsg}
            </span>
          </div>
        )}
      </section>

      <section className="settings-group">
        <div className="settings-row">
          <span className="settings-label">显示 / 隐藏岛</span>
          <kbd>Alt + Q</kbd>
        </div>
        <div className="settings-row">
          <span className="settings-label">快速添加任务</span>
          <kbd>Alt + N</kbd>
        </div>
        <div className="settings-row">
          <span className="settings-label">停靠</span>
          <span className="settings-hint">胶囊右键菜单或托盘菜单</span>
        </div>
      </section>

      <WebhookSettings />

      <footer className="settings-about">
        weldd v0.1.0 · Windows 灵动岛待办
        <br />
        数据保存在本地 SQLite
      </footer>
    </div>
  );
}
