# AGENTS.md — weldd 项目约定

**weldd** 是 Windows 10/11 桌面"灵动岛"(Dynamic Island)待办应用:透明无边框置顶胶囊岛 + 今日/专注清单 + 定时提醒。完整方案见 `docs/DESIGN.md`(技术选型、数据模型、里程碑、风险对策)。代码在 `weldd/` 子目录。

与用户交流、代码注释、文档一律使用**简体中文**。

## 范围红线

- **M3(专注计时/倒计时/专注记录)按用户要求永久跳过**,不要"顺手"补回。胶囊中间显示"今日 N 项 · 下一项:xxx",面板顶部是"今日概览卡";`focus_sessions` 表已在迁移中建好留待 v2。
- 深浅色跟随、云同步、全屏让位等仍是 v2 候选,见 DESIGN.md §10。

## 构建与运行(Windows,Git Bash)

环境:Node 22 / pnpm 11 / Rust 1.98 MSVC / VS2022 BuildTools。crates.io 走 rsproxy.cn 镜像(已配好,勿改)。

```bash
cd weldd

pnpm install            # 首次
pnpm build              # 前端 tsc + vite
pnpm tauri dev          # 开发运行

# 完整构建(需要 MSVC 链接器,不要直接在 Git Bash 里跑 cargo build)
cmd //c build-debug.bat     # debug 链接构建
cmd //c build-release.bat   # release + NSIS 安装包
```

**坑**:Git Bash 与裸 cmd 里都没有 cargo PATH;MSVC 环境必须经 vcvars64。两个 .bat 内部处理了这些,构建一律走它们。cargo check 不需要链接器,可直接跑(需 `export PATH="/c/Users/Administrator/.cargo/bin:$PATH"`)。

## 验证方式

- `cargo check`(src-tauri 内)+ `pnpm build` 必须零 error。
- 运行时几何断言:`powershell -NoProfile -ExecutionPolicy Bypass -File geometry-test.ps1 -ExePath <weldd.exe 路径>`,断言首启 380×64、主显示器工作区顶部居中、顶部留白 8px。
- **合成鼠标驱动不了拖动循环**;拖动/展开动画/停靠/提醒到达等交互由用户现场实测反馈,不要试图用合成输入自动化它们。
- 检查二进制是否包含某功能时注意:优化器会把短字符串(如 `ui_opacity`)内联成立即数,`indexOf` 搜不到不代表功能缺失。

## 架构约定(改代码前必读)

- **岛形态的单一事实来源在 Rust 侧**(`island.rs` 的 `IslandState`),前端只通过 `set_island_mode` 命令请求、经 `island://mode` 事件回推同步。形态:折叠胶囊 380×64 / 展开面板 420×580 / 左右停靠把手 16×100 / 顶部停靠把手 100×16(窗口逻辑尺寸,均含阴影留边)。拖到顶部松手自动收缩为顶部把手,受 `top_dock` 设置开关控制(默认开,设置页可关)。
- **Windows 的 set_position/set_size 是异步的**:写完立刻回读做锚点会漂移。锚点一律取 `island.rs` 的 `LAST_GEOM` 缓存(目标几何),init/reset 路径直接按目标值计算,禁止"写后读"。
- **前端绝不能调 `window.close()` 隐藏岛**(会销毁窗口),隐藏必须走 Rust 命令 `hide_island`。
- **`USER_DRAGGED` 原子标记**:只有真实拖动(drag_island 触发)后的"停稳"才参与贴边吸附与位置保存,防止把手滑出等程序性移动被误判。贴边吸附阈值 36 逻辑 px。
- 数据:SQLite(rusqlite, WAL)在 `%APPDATA%/com.weldd.app/weldd.db`,主连接在 `State<Db>`,提醒线程自开第二条连接。任务变更后必须 `emit("tasks://changed")` + `reminder::invalidate()`,前端统一 refetch。
- 轻量配置(岛位置/形态/透明度)走 tauri-plugin-store 的 `settings.json`;前端可写的键由 `island::UI_SETTING_KEYS` 白名单控制。
- 前端状态:zustand(`src/stores/tasks.ts` 数据缓存、`ui.ts` 形态/设置页/Alt+N 信号)。跨"组件挂载时机"的事件(quick-add)用自增序号而不是直接 listen,避免事件早于挂载丢失。
- 动画全部用 CSS + Rust 步进 resize,**没有引入 framer-motion**(刻意决策,效果等价)。
- Rust 代码注释保持现有密度与风格:只解释"为什么"(约束、坑),不复述代码。
