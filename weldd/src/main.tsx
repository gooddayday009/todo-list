import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import ReminderDialog from "./components/ReminderDialog";
import "./index.css";

// 到期提醒对话框是 Rust 侧按需创建的独立全屏窗口,与岛共用同一前端入口,
// 按 query 分流(见 reminder.rs 的 WebviewUrl)
const isReminder =
  new URLSearchParams(window.location.search).get("window") === "reminder";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {isReminder ? <ReminderDialog /> : <App />}
  </React.StrictMode>,
);
