# DeepPi

Tauri v2 桌面宿主：单窗口内承载 Pi Coding Agent 与 DeepSeek Harness 两个工作区，另有任务看板、四象限清单、桌宠等辅助浮窗。

## Language

### 工作区

**工作区 (Workspace)**:
同一主窗口内二选一的顶层工作界面，只有 Pi 工作区与 DSH 工作区两种。
_Avoid_: 模式、视图、agent 页

**Pi 工作区**:
Pi Coding Agent 的工作区，以项目与任务组织对话和终端。
_Avoid_: Pi 模式、主页

**DSH 工作区**:
承载 DeepSeek Harness 的工作区；其内容运行在独立原生 Webview 中，HTML 界面无法覆盖其上。
_Avoid_: DSH 模式、浏览器区

**工作区切换器 (Workspace Switcher)**:
顶栏左侧的分段控件，在两个工作区间一键切换；是工作区切换的唯一常驻入口。
_Avoid_: 侧边栏切换、控制柱切换

### 账户额度

**账户额度 (Account Quota)**:
所选模型对应供应商账户可查询的余额或套餐剩余用量；仅在供应商提供获授权的查询结果时才有数值。与单次对话 Token 统计无关。
_Avoid_: 会话消耗、上下文占用、并发额度

**并发额度 (Concurrency Limit)**:
Pi 工作区允许同时执行任务的会话数量限制，不代表任何供应商账户的剩余用量。
_Avoid_: 账户余额、套餐用量

### Pi 会话

**停止当前响应 (Interrupt Response)**:
在同一 Pi 会话内中断当前生成或执行并清空待发送队列；会话保持可继续对话。输入框中的“停止任务”是此操作的可见记录，不代表关闭整个任务。
_Avoid_: 关闭会话、结束任务

### 窗口结构

**顶栏 (Topbar)**:
窗口顶部常驻的横向 chrome，自左向右为工作区切换器、上下文面包屑、工具按钮区。
_Avoid_: 工具栏、菜单栏

**项目侧栏 (Project Sidebar)**:
Pi 工作区左侧列出项目与任务的侧栏。
_Avoid_: 侧边栏（单用时歧义）、任务栏

**文件栏 (Files Panel)**:
Pi 工作区右侧的文件树面板。
_Avoid_: 文件侧边栏

**Git 变更栏 (Git Panel)**:
Pi 工作区最右列出 Git 变更的侧栏。
_Avoid_: Git 侧边栏

### 已移除

**控制柱 (Command Rail)**:
曾驻留窗口左侧的竖向图标柱，承载工作区切换与设置入口，并曾在 DSH 工作区收成 8px 窄条、鼠标经过即弹出；因误触发已决定整条移除（见 ADR-0001），职责由顶栏接管。
_Avoid_: 侧边栏、工具条、rail
