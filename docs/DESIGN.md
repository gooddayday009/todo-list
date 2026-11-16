# weldd — Windows 灵动岛(Dynamic Island)方案设计

> 版本 v0.1 · 2026-09-08 · 目标平台 Windows 10/11 x64

## 1. 产品概述

**weldd** 是一个常驻 Windows 桌面的"灵动岛"小组件宿主:

- 顶部悬浮**胶囊形毛玻璃岛**:透明、无边框、始终置顶、不出现在任务栏
- 支持**折叠(胶囊)/ 展开(面板)/ 边缘停靠 / 托盘隐藏**四种存在形态
- 第一个内置功能:**Todo 清单** —— 今日任务、专注任务(番茄钟式计时)、定时提醒
- 架构上把"岛"设计为**模块宿主**,后续可挂天气、音乐控制、剪贴板等模块,不在本期范围

视觉基准(见设计图):胶囊全圆角、半透明银白毛玻璃、左侧绿色圆形图标、白色应用名、青色倒计时文字、右侧圆形操作钮。

## 2. 技术选型

| 层 | 选择 | 理由 |
|---|---|---|
| 框架 | **Tauri 2.x**(Rust + WebView2) | 原生支持透明/无边框/置顶/跳过任务栏;安装包 3–5MB、内存约 60–90MB(Electron 同等约 300MB+);计时与提醒放 Rust 侧不受 WebView 节流影响 |
| 前端 | Vite + **React 18** + TypeScript + Tailwind CSS + zustand + framer-motion | 组件生态成熟,动效表达力好;岛 UI 体量小,构建产物小 |
| 存储 | **rusqlite**(SQLite,Rust 侧管理) | 提醒调度器本来就在 Rust 侧读任务表,单一数据入口;前端经 commands/events 访问 |
| 插件 | notification / global-shortcut / autostart / single-instance / store | Toast 提醒、全局快捷键、开机自启、单实例、轻量配置持久化 |

## 3. 窗口架构(核心设计)

### 3.1 单窗口 + 动态尺寸

岛 = 一个 `decorations:false / transparent:true / always_on_top:true / skip_taskbar:true / shadow:false` 的 Tauri 窗口。三种几何形态对应**三种物理窗口尺寸**,形态切换时由 Rust 侧 `set_size + set_position`:

| 形态 | 窗口尺寸(含 12px 阴影留边) | 可见内容 |
|---|---|---|
| 折叠胶囊 pill | 380 × 56 | 约 356×44 胶囊:绿点图标 / 任务名+倒计时 / 圆形按钮 |
| 展开面板 panel | 420 × 560 | 专注卡 + Tab[今日\|专注] + 任务列表 + 输入框 |
| 边缘停靠 docked | 16 × 100(左右)/ 100 × 16(顶部) | 半透明小把手,贴屏幕左/右缘或顶缘 |

**为什么不用"一个大透明窗口、内容在内部缩放"**:Windows 上透明区域仍会命中鼠标事件,一个大窗口会挡住其下方的桌面和应用。动态 resize 让窗口真实边界始终贴合可见内容,零遮挡。

代价是原生 resize 无动画 —— 由 Rust 侧做 **60fps 步进 resize(约 160ms 缓动)** 补偿,配合内容 CSS 过渡,视觉等效"展开动画"。

### 3.2 形态状态机

```
collapsed ⇄ expanded   hover 进入(150ms 防抖)/ 鼠标离开面板 300ms 回收
collapsed ⇄ docked     右键菜单或快捷键触发;hover 把手 1s 滑出
any       ⇄ hidden     托盘隐藏(仅托盘图标);双击托盘 / Alt+Q 恢复
```

- 默认位置:主显示器顶部水平居中,顶部留 8px;可拖动(拖动把手区域),位置持久化到 store
- 移动后按**当前显示器工作区**夹紧,支持多显示器拖放
- 置顶但不抢焦点:默认 `focusable:false`,点击才临时获取焦点,避免打字时被抢焦点;v2 增加"前台全屏应用(游戏/放映)时自动隐藏"(Rust 侧监听前台窗口)

### 3.3 环境检查结论(2026-09-08 实测)

| 依赖 | 状态 |
|---|---|
| Node.js | ✅ v22.22.0 |
| pnpm | ✅ 11.22.0 |
| WebView2 Runtime | ✅ 147.0.3912.72 |
| Rust(rustc/cargo) | ✅ 1.98.1 stable-x86_64-pc-windows-msvc(2026-09-08 安装) |
| VS C++ Build Tools | ✅ 2022 BuildTools + VCTools 工作负载(MSVC v143,`C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools`) |

> 网络备注:当前网络直连 `static.rust-lang.org` / `crates.io` 会 TLS 握手失败,已配置 rsproxy.cn 镜像(rustup 环境变量 + `~/.cargo/config.toml`),工具链编译已实测通过。

## 4. 交互与视觉

### 4.1 折叠胶囊(默认态,按设计稿)

- 形状:全圆角(半径 = 高/2);材质:`rgba(255,255,255,0.14)` + `backdrop-filter: blur(28px) saturate(170%)`,1px 内描边 `rgba(255,255,255,0.28)`,柔和外投影
- 左:绿色圆形图标 `#30D158`,专注计时中带呼吸光晕
- 中:当前专注任务标题(白 95%)+ 倒计时青色 `#64D2FF`,等宽数字(`font-variant-numeric: tabular-nums`);无专注时显示"今日 N 项 · 下一项:xxx"
- 右:圆形按钮(无专注=开始专注;有专注=暂停/继续)
- 提醒到达:岛轻微弹跳 + 红点呼吸 + Toast 通知

### 4.2 展开面板

- 顶部**专注卡**:大号进度环 + 倒计时 + 暂停/放弃按钮(无专注时显示"选择任务开始专注")
- Tab 切换:**今日**(今日待办)/ **专注**(标记为专注清单的任务)
- 任务行:完成勾选框、优先级色点、标题、提醒铃铛(有提醒时间则显示时刻)、右侧 ⋯ 菜单(编辑/设提醒/移入专注/删除)
- 底部常驻输入框:"添加任务,回车保存",`Tab` 键切换日期/提醒快捷语法(如 `14:30` 自动设提醒)
- 拖动把手:胶囊右侧按钮长按或面板顶部边缘

### 4.3 全局快捷键(默认,可在设置改)

| 键 | 功能 |
|---|---|
| `Alt+Q` | 显示 / 隐藏岛 |
| `Alt+F` | 开始 / 暂停当前专注 |
| `Alt+N` | 快速添加任务(展开面板并聚焦输入框) |

### 4.4 托盘

- 图标:绿色圆点(与岛 logo 一致);**左键单击** = 显示/隐藏岛,**右键** = 菜单
- 菜单:显示/隐藏岛 · 边缘停靠 · 开机自启 ✓ · 设置 · 退出

## 5. 数据模型

```sql
-- tasks:任务主表
CREATE TABLE tasks (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  title       TEXT NOT NULL,
  note        TEXT DEFAULT '',
  status      INTEGER NOT NULL DEFAULT 0,      -- 0待办 1已完成
  list        TEXT NOT NULL DEFAULT 'today',   -- 'today' | 'focus'
  date        TEXT NOT NULL,                   -- 归属日 YYYY-MM-DD
  remind_at   INTEGER,                         -- Unix ms,可空
  remind_fired_at INTEGER,                     -- 防重复触发
  focus_minutes INTEGER NOT NULL DEFAULT 25,
  sort_order  INTEGER NOT NULL DEFAULT 0,
  created_at  INTEGER NOT NULL,
  completed_at INTEGER
);

-- focus_sessions:专注记录(供统计与恢复)
CREATE TABLE focus_sessions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  started_at INTEGER NOT NULL,
  ended_at INTEGER,
  planned_minutes INTEGER NOT NULL,
  completed INTEGER NOT NULL DEFAULT 0
);
```

轻量配置(岛位置/形态记忆、专注时长、短休时长、提醒声音、快捷键、开机自启)走 `tauri-plugin-store` 的 JSON,不进 SQLite。

### 5.1 数据导出 / 导入

设置页"数据备份"提供 JSON 备份(命令 `export_tasks` / `import_tasks`,对话框走 `tauri-plugin-dialog` 的 Rust API):

- **导出**:全部任务(含历史)写为 `weldd-backup-时间戳.json`,文件头 `{ format: "weldd-export", version: 1 }`,导入时校验防误选。
- **导入**:合并语义,不做覆盖。以 `(created_at, title)` 判重,库里已有的跳过;新任务保留提醒、完成状态、正文,以新 id 插入(单事务,失败整体回滚)。这样换机/重装后导入不会产生重复,同一份备份导两次也幂等。

## 6. 核心机制(Rust 侧)

### 6.1 专注计时器(`focus.rs`)

- `start_focus(task_id, minutes)`:tokio 异步任务每 250ms 一次 tick,`emit("focus://tick", {remaining_ms})` 推给前端驱动 UI
- 剩余时间一律用**绝对时间差**(开始时刻 + 时长 − now)计算,不累计 tick,天然免疫睡眠唤醒偏差与调度抖动
- 到点:`emit("focus://done")` + 系统通知 + 提示音;写一条 `focus_sessions`
- 暂停/恢复/放弃:重设基准时刻并重新调度;应用重启时若有未完成 session,标记中断并提示

### 6.2 提醒调度器(`reminder.rs`)

- 内存维护按 `remind_at` 排序的队列;调度线程 `sleep` 到最近一个到期点,醒来逐个触发 Toast + `emit("reminder://due")` + 岛动效
- 任务增删改 → 重建队列;**每 30s 兜底校验**一次,覆盖系统睡眠、时钟调整
- 唤醒/启动时扫描已过期且未触发的提醒 → 一次性补发"错过的提醒"摘要
- 触发成功写 `remind_fired_at` 防重复

### 6.3 岛窗口控制(`island.rs`)

- 形态状态机:set_size/set_position 的步进动画(约 160ms、easeOutCubic)、位置夹紧、边缘停靠/滑出
- 前端通过 command(`set_island_mode(mode)`)与事件(`island://mode`)双向同步

## 7. 工程结构

```
weldd/
├─ package.json / pnpm-lock.yaml
├─ vite.config.ts / tailwind.config.ts / tsconfig.json
├─ src/                          # 前端(React)
│  ├─ app/                       # IslandApp 入口、形态状态机 hook
│  ├─ components/                # Pill / FocusCard / TaskList / TaskItem / AddBar / ...
│  ├─ stores/                    # useTaskStore / useFocusStore / useIslandStore
│  ├─ lib/                       # tauri bridge(invoke/listen 封装)
│  └─ styles/
└─ src-tauri/                    # Rust
   ├─ tauri.conf.json            # 窗口与打包配置、能力声明
   ├─ icons/
   └─ src/
      ├─ lib.rs                  # 装配:插件、托盘、commands 注册
      ├─ island.rs               # 窗口几何状态机、步进 resize、停靠
      ├─ focus.rs                # 专注计时
      ├─ reminder.rs             # 提醒调度
      ├─ db.rs                   # rusqlite:migration + CRUD
      ├─ tray.rs                 # 托盘
      └─ commands.rs             # 暴露给前端的命令
```

## 8. 里程碑

| 阶段 | 内容 | 预估 |
|---|---|---|
| M0 环境就绪 | 安装 rustup(MSVC)+ VS Build Tools;`pnpm create tauri-app` 脚手架跑通 | 0.5 天 |
| M1 岛骨架 | 透明/无边框/置顶胶囊 + hover 展开/收起 + 步进动画 + 拖动 + 托盘 + 隐藏,静态视觉还原设计稿 | 1–2 天 |
| M2 任务数据 | SQLite 迁移 + 今日/专注清单 CRUD + 展开面板 UI | 1–2 天 |
| M3 专注模式 | Rust 计时 + 胶囊倒计时 + 完成通知/声音 + 专注记录 | 1–2 天 |
| M4 定时提醒 | 调度器 + Toast + 睡眠唤醒补发 + 岛提醒动效 | 1 天 |
| M5 打磨 | 边缘停靠、开机自启、快捷键、多显示器夹紧、设置面板、NSIS 安装包 | 1–2 天 |

## 9. 风险与对策

| 风险 | 对策 |
|---|---|
| 部分 Win10 旧版本透明窗口偶发白底 | 检测 WebView2/系统版本,提供"纯色圆角"降级开关 |
| 步进 resize 抖动 | 只在 折叠⇄展开 两档间切换 + 内容 CSS 过渡掩盖;不做中间形态 |
| 置顶窗口抢焦点 | 默认 `focusable:false`,点击才临时获焦 |
| 睡眠唤醒后计时偏差 | 剩余时间用绝对时间差计算,30s 兜底校验 + 唤醒补发 |
| 提醒在应用被杀后丢失 | 启动时扫描过期未触发任务补发;v2 可评估 Windows 计划任务拉起 |

## 10. 暂不做(v2 候选)

全屏自动让位、深浅色模式跟随、数据云同步、专注统计图表、更多岛模块(天气/音乐/剪贴板)、多语言。
