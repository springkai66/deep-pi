# 删除控制柱，工作区切换上移顶栏

DSH 工作区曾把左侧控制柱收成 8px 窄条、鼠标经过零延迟弹出：路过窗口左缘即误触发，且 workspace 上的 ResizeObserver 会逐帧把原生 Webview 随栅格动画来回拽动。我们决定在两种工作区下把控制柱整条删除，工作区切换改由顶栏左侧的分段控件承载，设置入口移至顶栏工具区，并新增 Ctrl+1/Ctrl+2 切换快捷键；顶栏高度保持 32px 不变，工作区/对话区不缩反增（Pi 内容区 +48px，DSH Webview +8px）。

Status: accepted

## Considered Options

- **悬停意图延迟（dwell ~200ms）**：只治误触发，不治「柱子占位 + Webview 被拽」，还给每次有意悬停加上延迟。
- **控制柱恒 48px 常驻**：无悬停陷阱，但 DSH 永久损失 40px 宽度，且为「切换 + 设置」两个入口养一条常驻竖列。
- **展开态 overlay 飞出面板**：HTML 会被原生子 Webview 遮挡，展开期间必须隐藏 Webview，造成内容闪烁。

## Consequences

- 原生子 Webview 覆盖 HTML，因此顶栏是 DSH 工作区唯一可交互的常驻 chrome；DSH 模式下任何新入口都必须放顶栏或走快捷键。
