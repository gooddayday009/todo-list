import { useEffect, useRef, useState, type KeyboardEvent, type RefObject } from "react";
import { fmtRemindLabel, parseQuickSyntax } from "../lib/tauri";
import { useTasks } from "../stores/tasks";
import { useUi } from "../stores/ui";

interface AddBarProps {
  onMaybeCollapse: () => void;
  pointerInside: RefObject<boolean>;
}

/** 已消费过的 Alt+N 信号序号(模块级:面板收起再展开时不重复聚焦) */
let handledQuickAddSeq = 0;

/** 底部常驻输入框:回车保存;`14:30` / `明天 14:30` 自动设提醒 */
export default function AddBar({ onMaybeCollapse, pointerInside }: AddBarProps) {
  const add = useTasks((s) => s.add);
  const mode = useUi((s) => s.mode);
  const quickAddSeq = useUi((s) => s.quickAddSeq);
  const [text, setText] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  // Alt+N:只聚焦新信号(晚于组件挂载也能收到;旧信号重挂载不触发)
  useEffect(() => {
    if (mode === "expanded" && quickAddSeq > handledQuickAddSeq) {
      handledQuickAddSeq = quickAddSeq;
      inputRef.current?.focus();
    }
  }, [quickAddSeq, mode]);

  // 输入框失焦且鼠标已离开窗口 → 回收面板(App 的 onMouseLeave 被聚焦态拦下了)
  const handleBlur = () => {
    if (!pointerInside.current) onMaybeCollapse();
  };

  const parsed = parseQuickSyntax(text);

  const submit = () => {
    const value = text.trim();
    if (!value) return;
    void add(parsed.title || value, parsed.remindAt);
    setText("");
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" && !e.nativeEvent.isComposing) submit();
    if (e.key === "Escape") inputRef.current?.blur();
  };

  return (
    <footer className="add-bar">
      <div className="add-input-wrap">
        <span className="add-icon" aria-hidden="true">+</span>
        <input
          ref={inputRef}
          type="text"
          value={text}
          placeholder="添加一件待办…"
          aria-label="添加任务"
          onChange={(e) => setText(e.target.value)}
          onKeyDown={onKeyDown}
          onBlur={handleBlur}
        />
        <button type="button" className="add-submit" title="添加任务" aria-label="添加任务" disabled={!text.trim()} onMouseDown={(e) => e.preventDefault()} onClick={submit}>↵</button>
      </div>
      <div className="add-helper"><span>输入 14:30 自动设置提醒</span><kbd>Alt + N</kbd></div>
      {parsed.remindAt && (
        <span className="add-remind-chip">⏰ {fmtRemindLabel(parsed.remindAt)}</span>
      )}
    </footer>
  );
}
