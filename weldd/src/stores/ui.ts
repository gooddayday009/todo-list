/** UI 状态:岛形态(Rust 回推)、快速添加信号、设置页/历史页开关 */

import { create } from "zustand";
import type { IslandMode } from "../lib/tauri";

interface UiState {
  mode: IslandMode;
  /** Alt+N 每次触发自增,AddBar 监听变化后聚焦(避免事件早于组件挂载丢失) */
  quickAddSeq: number;
  settingsOpen: boolean;
  historyOpen: boolean;
  setMode: (mode: IslandMode) => void;
  bumpQuickAdd: () => void;
  setSettingsOpen: (open: boolean) => void;
  setHistoryOpen: (open: boolean) => void;
}

export const useUi = create<UiState>((set) => ({
  mode: "collapsed",
  quickAddSeq: 0,
  settingsOpen: false,
  historyOpen: false,
  setMode: (mode) => set({ mode }),
  bumpQuickAdd: () => set((s) => ({ quickAddSeq: s.quickAddSeq + 1 })),
  // 设置页与历史页互斥:打开一个就关掉另一个
  setSettingsOpen: (settingsOpen) => set({ settingsOpen, historyOpen: false }),
  setHistoryOpen: (historyOpen) => set({ historyOpen, settingsOpen: false }),
}));
