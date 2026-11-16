import { useEffect, useRef, useState, type ClipboardEvent, type KeyboardEvent, type MouseEvent } from "react";
import { fmtRemindLabel, type Task } from "../lib/tauri";
import { NOTE_MAX_LEN, emptyToNull, noteTextLength, sanitizeNoteHtml } from "../lib/note";
import { useTasks } from "../stores/tasks";

interface TaskListProps {
  tasks: Task[];
  emptyText: string;
  emptyHint: string;
}

/** 任务列表:完成勾选 / 标题 / 正文预览 / 提醒时刻 / ⋯ 菜单(编辑/设提醒/删除) */
export default function TaskList({ tasks, emptyText, emptyHint }: TaskListProps) {
  if (tasks.length === 0) {
    return (
      <div className="task-empty">
        <div className="empty-glyph" aria-hidden="true" />
        <p className="empty-title">{emptyText}</p>
        <p className="empty-hint">{emptyHint}</p>
      </div>
    );
  }

  return (
    <div className="task-list">
      {tasks.map((task) => (
        <TaskRow key={task.id} task={task} />
      ))}
    </div>
  );
}

function TaskRow({ task }: { task: Task }) {
  const toggle = useTasks((s) => s.toggle);
  const rename = useTasks((s) => s.rename);
  const remove = useTasks((s) => s.remove);
  const setReminder = useTasks((s) => s.setReminder);

  const [menuOpen, setMenuOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  const [remindEditing, setRemindEditing] = useState(false);
  const [noteEditing, setNoteEditing] = useState(false);
  // 浮层默认弹在行下方,行贴近列表底部时改朝上,避免被滚动容器裁剪
  const [noteFlipUp, setNoteFlipUp] = useState(false);
  const rowRef = useRef<HTMLDivElement>(null);

  const done = task.status === 1;
  // 过期未处理:提醒已触发但仍待办(与胶囊的持久过期提示同一判定)
  const overdue = !done && task.remindAt !== null && task.remindFiredAt !== null;

  // 点击行内其他地方关闭 ⋯ 菜单
  useEffect(() => {
    if (!menuOpen) return;
    const close = (e: globalThis.MouseEvent) => {
      if (!rowRef.current?.contains(e.target as Node)) setMenuOpen(false);
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [menuOpen]);

  const startEdit = () => {
    setMenuOpen(false);
    setRemindEditing(false);
    setEditing(true);
  };

  const startRemindEdit = () => {
    setMenuOpen(false);
    setEditing(false);
    setNoteEditing(false);
    setRemindEditing(true);
  };

  const startNoteEdit = () => {
    setMenuOpen(false);
    setEditing(false);
    setRemindEditing(false);
    const list = rowRef.current?.closest(".task-list");
    if (rowRef.current && list) {
      const r = rowRef.current.getBoundingClientRect();
      const lr = list.getBoundingClientRect();
      setNoteFlipUp(r.bottom > lr.bottom - 180);
    }
    setNoteEditing(true);
  };

  const commitTitle = () => {
    const input = rowRef.current?.querySelector<HTMLInputElement>(".task-edit-input");
    const title = input?.value.trim() ?? "";
    if (title && title !== task.title) void rename(task.id, title);
    setEditing(false);
  };

  /** 本地时间输入值:已有提醒取其时刻,否则默认 1 小时后 */
  const remindInputValue = () => {
    const d = task.remindAt ? new Date(task.remindAt) : new Date(Date.now() + 3600_000);
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(
      d.getHours(),
    )}:${pad(d.getMinutes())}`;
  };

  const commitRemind = () => {
    const date = rowRef.current?.querySelector<HTMLInputElement>(".task-remind-input")?.value;
    const hour = rowRef.current?.querySelector<HTMLSelectElement>(".task-remind-hour")?.value;
    const minute = rowRef.current?.querySelector<HTMLSelectElement>(".task-remind-minute")?.value;
    if (!date || hour === undefined || minute === undefined) return;
    const raw = `${date}T${hour}:${minute}`;
    const ts = new Date(raw).getTime(); // 无时区后缀 → 按本地时区解析
    if (Number.isNaN(ts)) return;
    void setReminder(task.id, ts);
    setRemindEditing(false);
  };

  const onEditKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") commitTitle();
    if (e.key === "Escape") setEditing(false);
  };

  const onRemindKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter") commitRemind();
    if (e.key === "Escape") setRemindEditing(false);
  };

  const stopPropagation = (e: MouseEvent) => e.stopPropagation();

  // 三击标题打开正文;三击的第三下可能落在刚替换出的标题输入框上,一并接住
  const maybeTripleClick = (e: MouseEvent) => {
    if (e.detail === 3) {
      e.preventDefault();
      startNoteEdit();
    }
  };

  return (
    <>
      <div className={`task-row ${done ? "task-done" : ""} ${overdue ? "task-overdue" : ""}`} ref={rowRef}>
        <button
          type="button"
          className={`task-check ${done ? "checked" : ""}`}
          title={done ? "标记为待办" : "完成"}
          onClick={() => void toggle(task)}
        >
          {done && (
            <svg width="10" height="10" viewBox="0 0 10 10" fill="none" aria-hidden="true">
              <path
                d="M1.5 5.2 4 7.7 8.5 2.6"
                stroke="#10241a"
                strokeWidth="1.8"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
          )}
        </button>

        <div className="task-main">
          {editing ? (
            <input
              className="task-edit-input"
              defaultValue={task.title}
              autoFocus
              onKeyDown={onEditKeyDown}
              onBlur={commitTitle}
              onMouseDown={stopPropagation}
              onClick={maybeTripleClick}
            />
          ) : (
            <span
              className="task-title"
              onDoubleClick={startEdit}
              onClick={maybeTripleClick}
              title="双击编辑标题,三击写正文"
            >
              {task.title}
            </span>
          )}
        </div>

        {remindEditing ? (
          <div
            className="task-remind-edit"
            onMouseDown={stopPropagation}
            onKeyDown={onRemindKeyDown}
            onBlur={(e) => {
              // 日期、小时和分钟之间切换焦点时,不能提前保存并关闭。
              if (!e.currentTarget.contains(e.relatedTarget)) commitRemind();
            }}
          >
            <input
              className="task-remind-input"
              type="date"
              aria-label="提醒日期"
              defaultValue={remindInputValue().slice(0, 10)}
              autoFocus
            />
            <select className="task-remind-hour" aria-label="提醒小时（24小时制）" defaultValue={remindInputValue().slice(11, 13)}>
              {Array.from({ length: 24 }, (_, n) => {
                const value = String(n).padStart(2, "0");
                return <option key={value} value={value}>{value}</option>;
              })}
            </select>
            <span aria-hidden="true">:</span>
            <select className="task-remind-minute" aria-label="提醒分钟" defaultValue={remindInputValue().slice(14, 16)}>
              {Array.from({ length: 60 }, (_, n) => {
                const value = String(n).padStart(2, "0");
                return <option key={value} value={value}>{value}</option>;
              })}
            </select>
          </div>
        ) : task.remindAt ? (
          <button
            type="button"
            className={`task-bell ${overdue ? "overdue" : task.remindFiredAt ? "fired" : ""}`}
            title={
              overdue
                ? `已过期(原定 ${fmtRemindLabel(task.remindAt)}),点击改期`
                : task.remindFiredAt
                  ? "已提醒"
                  : `提醒:${fmtRemindLabel(task.remindAt)}(点击修改)`
            }
            onClick={startRemindEdit}
          >
            <svg width="10" height="10" viewBox="0 0 12 12" fill="none" aria-hidden="true">
              <path
                d="M6 1.2a3.4 3.4 0 0 0-3.4 3.4c0 2.6-1 3.4-1 3.4h8.8s-1-.8-1-3.4A3.4 3.4 0 0 0 6 1.2ZM4.8 9.6a1.3 1.3 0 0 0 2.4 0"
                stroke="currentColor"
                strokeWidth="1.1"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
            <span>{fmtRemindLabel(task.remindAt)}</span>
          </button>
        ) : null}

        <div className="task-menu-anchor">
          <button
            type="button"
            className="task-more"
            title="更多操作"
            onClick={() => setMenuOpen((v) => !v)}
          >
            <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
              <circle cx="2.5" cy="6" r="1.2" fill="currentColor" />
              <circle cx="6" cy="6" r="1.2" fill="currentColor" />
              <circle cx="9.5" cy="6" r="1.2" fill="currentColor" />
            </svg>
          </button>

          {menuOpen && (
            <div className="task-menu">
              <button type="button" onClick={startEdit}>
                编辑标题
              </button>
              <button type="button" onClick={startNoteEdit}>
                {task.note ? "编辑正文" : "写正文"}
              </button>
              <button type="button" onClick={startRemindEdit}>
                {task.remindAt ? "修改提醒" : "设提醒"}
              </button>
              {task.remindAt && (
                <button type="button" onClick={() => void setReminder(task.id, null)}>
                  清除提醒
                </button>
              )}
              <button
                type="button"
                className="danger"
                onClick={() => {
                  setMenuOpen(false);
                  void remove(task.id);
                }}
              >
                删除
              </button>
            </div>
          )}
        </div>

        {noteEditing && <NoteEditor task={task} flipUp={noteFlipUp} onClose={() => setNoteEditing(false)} />}
      </div>
    </>
  );
}

const NOTE_TOOLS = [
  { cmd: "bold", label: "B", cls: "b" },
  { cmd: "italic", label: "I", cls: "i" },
  { cmd: "underline", label: "U", cls: "u" },
  { cmd: "strikeThrough", label: "S", cls: "s" },
] as const;

/** 正文编辑器:contenteditable + B/I/U/S 工具栏,浮在任务行右侧,失焦保存、Esc 取消 */
function NoteEditor({ task, flipUp, onClose }: { task: Task; flipUp: boolean; onClose: () => void }) {
  const setNote = useTasks((s) => s.setNote);
  const wrapRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<HTMLDivElement>(null);
  const cancelledRef = useRef(false);
  // 最近一次未超限的 HTML 快照,超限时回滚用
  const lastGoodRef = useRef(task.note);
  const [count, setCount] = useState(() => noteTextLength(task.note));
  const [fmt, setFmt] = useState({ bold: false, italic: false, underline: false, strike: false });

  useEffect(() => {
    const el = editorRef.current;
    if (!el) return;
    el.innerHTML = sanitizeNoteHtml(task.note);
    el.focus();
    const sel = window.getSelection();
    const range = document.createRange();
    range.selectNodeContents(el);
    range.collapse(false); // 光标落到末尾,方便接着写
    sel?.removeAllRanges();
    sel?.addRange(range);
  }, []);

  const placeCaretEnd = (el: HTMLDivElement) => {
    const sel = window.getSelection();
    const range = document.createRange();
    range.selectNodeContents(el);
    range.collapse(false);
    sel?.removeAllRanges();
    sel?.addRange(range);
  };

  const syncCount = () => {
    const el = editorRef.current;
    if (!el) return;
    const len = noteTextLength(el.innerHTML);
    if (len > NOTE_MAX_LEN) {
      el.innerHTML = sanitizeNoteHtml(lastGoodRef.current);
      placeCaretEnd(el);
      setCount(NOTE_MAX_LEN);
      return;
    }
    lastGoodRef.current = el.innerHTML;
    setCount(len);
  };

  const commit = () => {
    if (cancelledRef.current) return;
    const el = editorRef.current;
    if (!el) return;
    const clean = emptyToNull(sanitizeNoteHtml(el.innerHTML));
    if (clean !== task.note) void setNote(task.id, clean);
    onClose();
  };

  const cancel = () => {
    cancelledRef.current = true;
    onClose();
  };

  const onPaste = (e: ClipboardEvent) => {
    e.preventDefault(); // 只收纯文本,外部富标记一律不进编辑器
    const el = editorRef.current;
    if (!el) return;
    const text = e.clipboardData.getData("text/plain");
    const remain = NOTE_MAX_LEN - noteTextLength(el.innerHTML);
    if (remain <= 0 || !text) return;
    document.execCommand("insertText", false, Array.from(text).slice(0, remain).join(""));
  };

  const exec = (cmd: string) => {
    document.execCommand(cmd);
    editorRef.current?.focus();
    syncCount();
  };

  // selectionchange 同步 B/I/U/S 按钮的高亮态
  useEffect(() => {
    const onSel = () => {
      const el = editorRef.current;
      const sel = window.getSelection();
      if (!el || !sel || !el.contains(sel.anchorNode)) return;
      try {
        setFmt({
          bold: document.queryCommandState("bold"),
          italic: document.queryCommandState("italic"),
          underline: document.queryCommandState("underline"),
          strike: document.queryCommandState("strikeThrough"),
        });
      } catch {
        // queryCommandState 在极端选区下会抛错,不影响编辑
      }
    };
    document.addEventListener("selectionchange", onSel);
    return () => document.removeEventListener("selectionchange", onSel);
  }, []);

  // 工具栏按钮经键盘/无障碍激活时,焦点会先落到按钮再被 exec 拉回编辑器;
  // 失焦不能立即提交,延迟一拍确认焦点确实离开了整个编辑器才保存
  const onEditorBlur = () => {
    window.setTimeout(() => {
      if (cancelledRef.current) return;
      const wrap = wrapRef.current;
      if (!wrap) return; // 已卸载 = 已提交/取消
      if (wrap.contains(document.activeElement)) return;
      commit();
    }, 120);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      cancel();
    }
  };

  return (
    <div className={`note-editor ${flipUp ? "flip-up" : ""}`} ref={wrapRef} onMouseDown={(e) => e.stopPropagation()}>
      <div className="note-toolbar">
        {NOTE_TOOLS.map((t) => (
          <button
            key={t.cmd}
            type="button"
            className={`note-tool ${t.cls} ${fmt[t.cmd === "strikeThrough" ? "strike" : t.cmd] ? "on" : ""}`}
            title={
              t.cmd === "bold"
                ? "加粗"
                : t.cmd === "italic"
                  ? "斜体"
                  : t.cmd === "underline"
                    ? "下划线"
                    : "删除线"
            }
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => exec(t.cmd)}
          >
            {t.label}
          </button>
        ))}
        <button
          type="button"
          className="note-tool"
          title="清除格式"
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => exec("removeFormat")}
        >
          Tx
        </button>
        <span className={`note-counter ${count >= NOTE_MAX_LEN - 20 ? "warn" : ""}`}>
          {count}/{NOTE_MAX_LEN}
        </span>
      </div>
      <div
        ref={editorRef}
        className="note-editable"
        contentEditable
        suppressContentEditableWarning
        role="textbox"
        aria-multiline="true"
        aria-label="任务正文"
        onKeyDown={onKeyDown}
        onInput={syncCount}
        onPaste={onPaste}
        onBlur={onEditorBlur}
      />
    </div>
  );
}
