import type { RefObject } from "react";
import { useTasks } from "../stores/tasks";
import { useUi } from "../stores/ui";
import OverviewCard from "./OverviewCard";
import TaskList from "./TaskList";
import AddBar from "./AddBar";
import SettingsView from "./SettingsView";
import HistoryView from "./HistoryView";

interface PanelProps {
  onMaybeCollapse: () => void;
  pointerInside: RefObject<boolean>;
}

/** 展开面板:概览卡 + 任务列表 + 输入框;设置页/历史报表页可切换 */
export default function Panel({ onMaybeCollapse, pointerInside }: PanelProps) {
  const settingsOpen = useUi((s) => s.settingsOpen);
  const historyOpen = useUi((s) => s.historyOpen);
  const tasks = useTasks((s) => s.tasks);

  if (settingsOpen) {
    return (
      <section className="panel">
        <SettingsView />
      </section>
    );
  }

  if (historyOpen) {
    return (
      <section className="panel">
        <HistoryView />
      </section>
    );
  }

  return (
    <section className="panel">
      <OverviewCard />

      <div className="list-heading">
        <h2>今日清单 <span>{tasks.length}</span></h2>
        <span>把重要的事，一件件完成</span>
      </div>
      <TaskList tasks={tasks} emptyText="给今天一个小目标" emptyHint="记下第一件事，从这里开始" />

      <AddBar onMaybeCollapse={onMaybeCollapse} pointerInside={pointerInside} />
    </section>
  );
}
