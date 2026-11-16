/** 任务数据 store:Rust 是事实来源,这里只是今日数据的本地缓存 */

import { create } from "zustand";
import { api, type Task } from "../lib/tauri";

interface TasksState {
  tasks: Task[];
  loaded: boolean;
  error: string | null;
  fetch: () => Promise<void>;
  add: (title: string, remindAt: number | null) => Promise<void>;
  rename: (id: number, title: string) => Promise<void>;
  setNote: (id: number, note: string) => Promise<void>;
  toggle: (task: Task) => Promise<void>;
  setReminder: (id: number, remindAt: number | null) => Promise<void>;
  remove: (id: number) => Promise<void>;
}

export const useTasks = create<TasksState>((set) => ({
  tasks: [],
  loaded: false,
  error: null,

  fetch: async () => {
    try {
      const tasks = await api.listTodayTasks();
      set({ tasks, loaded: true, error: null });
    } catch (e) {
      set({ loaded: true, error: String(e) });
    }
  },

  add: async (title, remindAt) => {
    await api.createTask(title, remindAt);
    // tasks://changed 事件会触发统一 refetch,这里不再重复拉
  },

  rename: async (id, title) => {
    await api.updateTaskTitle(id, title);
  },

  setNote: async (id, note) => {
    await api.updateTaskNote(id, note);
  },

  toggle: async (task) => {
    await api.setTaskStatus(task.id, task.status === 0);
  },

  setReminder: async (id, remindAt) => {
    await api.setTaskReminder(id, remindAt);
  },

  remove: async (id) => {
    await api.deleteTask(id);
  },
}));

/** 今日清单派生数据(胶囊与面板共用) */
export function selectToday(tasks: Task[]) {
  const open = tasks.filter((t) => t.status === 0);
  const done = tasks.filter((t) => t.status === 1);
  // 过期未处理:提醒已触发但仍待办。Rust 侧 mark_fired 是事实来源,
  // 完成任务或改期(remind_fired_at 被清)后自动退出该状态
  const overdue = open.filter((t) => t.remindAt !== null && t.remindFiredAt !== null);
  const next =
    [...open].sort(
      (a, b) => (a.remindAt ?? Number.MAX_SAFE_INTEGER) - (b.remindAt ?? Number.MAX_SAFE_INTEGER),
    )[0] ?? null;
  const nextRemind = open
    .map((t) => t.remindAt)
    .filter((v): v is number => v !== null)
    .sort((a, b) => a - b)[0];
  return { open, done, next, nextRemind, overdue };
}
