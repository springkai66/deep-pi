<script lang="ts">
  // 夹具：核对顶栏工作区切换器（ADR-0001：控制柱已删除，不存在任何悬停展开路径）。
  import "../../src/app.css";

  let activeAgent = $state<"pi" | "dsh">("dsh");
  let settingsOpen = $state(false);
</script>

<p>
  <output aria-label="当前工作区">{activeAgent === "pi" ? "Pi 工作区" : "DSH 工作区"}</output>
  <output aria-label="设置状态">{settingsOpen ? "设置打开" : "设置关闭"}</output>
</p>

<div class="app-shell dsh-mode" style="--sidebar-w:260px; --files-w:260px; --git-w:300px">
  <header class="topbar">
    <!-- 工作区切换器：两种工作区下位置恒定，替代已删除的控制柱（ADR-0001）。 -->
    <div class="workspace-switcher" role="group" aria-label="工作区切换">
      <button type="button" class:selected={activeAgent === "pi"}
        aria-label="Pi 工作区" aria-pressed={activeAgent === "pi"}
        title="Pi 工作区" onclick={() => { activeAgent = "pi"; }}>🤖</button>
      <button type="button" class:selected={activeAgent === "dsh"}
        aria-label="DSH 工作区" aria-pressed={activeAgent === "dsh"}
        title="DSH 工作区" onclick={() => { activeAgent = "dsh"; }}>🌐</button>
    </div>
    <div class="stage-crumbs">
      <span class="crumb-project"><span class="crumb-dot"></span>{activeAgent === "pi" ? "示例项目" : "DeepSeek Harness"}</span>
    </div>
    <div class="toolbar">
      <button type="button" aria-label="打开应用设置" aria-pressed={settingsOpen}
        title="设置 (Ctrl+,)" onclick={() => { settingsOpen = !settingsOpen; }}>⚙</button>
    </div>
  </header>
  <main class="workspace" style="border: 1px dashed var(--border-strong); display: grid; place-items: center;">
    <p>{activeAgent === "dsh" ? "DSH 原生 Webview 占位区（顶栏以下全部宽度）" : "Pi 工作区内容"}</p>
  </main>
</div>
