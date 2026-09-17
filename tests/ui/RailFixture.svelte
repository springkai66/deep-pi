<script lang="ts">
  // 夹具：模拟窗口以核对 DSH 模式下控制柱的收起/悬停展开。
  import "../../src/app.css";

  let dshMode = $state(true);
  let railExpanded = $state(false);
  const railCollapsed = $derived(dshMode && !railExpanded);
  let railState = $state("收起");
</script>

<p>
  <output aria-label="控制柱状态">{railState}</output>
  <button type="button" onclick={() => { dshMode = !dshMode; railState = dshMode ? "收起" : "Pi 模式"; }}>
    切换到 {dshMode ? "Pi 工作区" : "DSH 工作区"}
  </button>
</p>

<div
  style={`--sidebar-w:260px; --files-w:260px; --git-w:300px${dshMode ? `; grid-template-columns:${railCollapsed ? 8 : 140}px minmax(0, 1fr)` : ""}`}
  class:dsh-mode={dshMode}
  class="app-shell"
>
  <nav
    class="command-rail"
    class:rail-collapsed={railCollapsed}
    aria-label="工作区导航"
    onmouseenter={() => { railExpanded = true; railState = "展开"; }}
    onmouseleave={() => { railExpanded = false; railState = "收起"; }}
    onfocusin={() => { railExpanded = true; railState = "展开"; }}
    onfocusout={() => { railExpanded = false; railState = "收起"; }}
  >
    <div class="rail-brand" title="DeepPi" aria-hidden="true">π</div>
    <div class="rail-divider"></div>
    <button type="button" class="rail-item selected" aria-label="Pi 工作区"><span>🤖</span><span class="rail-label">Pi 工作区</span></button>
    <button type="button" class="rail-item" aria-label="DSH 工作区"><span>🌐</span><span class="rail-label">DSH 工作区</span></button>
    <div class="rail-spacer"></div>
    <button type="button" class="rail-item" aria-label="设置"><span>⚙</span><span class="rail-label">设置</span></button>
  </nav>
  <header class="topbar">
    <div class="stage-crumbs"><span class="crumb-project"><span class="crumb-dot"></span>DeepSeek Harness</span></div>
  </header>
  <main class="workspace" style="border: 1px dashed var(--border-strong); display: grid; place-items: center;">
    <p>DSH 原生 Webview 占位区</p>
  </main>
</div>
