<script lang="ts">
  import TaskSidebar from "../../src/lib/TaskSidebar.svelte";
  import FileSidebar from "../../src/lib/FileSidebar.svelte";
  import ChatPane from "../../src/lib/ChatPane.svelte";
  import AppDialog from "../../src/lib/AppDialog.svelte";
  import SettingsFixture from "./SettingsFixture.svelte";

  const noop = () => {};
  const project = { id: "glass-project", name: "透明主题项目", path: "F:/fixture", createdAt: 0, lastOpenedAt: 0 };
</script>

<!-- Match the main window's outer panels; all inner surfaces are real components. -->
<div class="app-shell">
  <aside class="project-sidebar">
    <TaskSidebar projects={[project]} tasks={[]} selectedProjectId={project.id}
      onAddProject={noop} onOpenProject={noop} onOpen={noop} onRename={noop}
      onStop={noop} onArchive={noop} onRestart={noop} onRestore={noop}
      onDelete={noop} onAddSession={noop} onRemoveProject={noop} />
  </aside>
  <main class="workspace">
    <ChatPane taskId="glass-task" runId={null} stopped title="透明主题对话"
      visible active switching={false} focusToken={0} autoName={false}
      onUseTerminal={noop} onDialog={async () => null} onCancelDialogs={noop}
      onActivity={noop} onAutoRename={() => false} onManualRename={async () => {}} />
  </main>
  <aside class="file-panel">
    <FileSidebar {project} onOpen={noop} onAction={async () => {}}
      visible searchFocusToken={0} refreshToken={0} />
  </aside>
  <div class="settings-modal"><SettingsFixture /></div>
</div>
<AppDialog request={{ id: 1, kind: "input", title: "透明主题弹窗", message: "弹窗底色也跟随透明度", resolve: noop }} onResolve={noop} />

<style>
  .app-shell { display: grid; grid-template-columns: 240px minmax(320px, 1fr) 260px; grid-template-rows: 380px 440px; height: auto; }
  .workspace { padding: 8px; }
  .settings-modal { grid-column: 1 / -1; min-height: 0; background: var(--page-bg); }
  .settings-modal :global(nav[aria-label="测试状态"]) { display: none; }
</style>
