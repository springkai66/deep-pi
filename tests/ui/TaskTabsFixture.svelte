<script lang="ts">
  import TaskTabs from "../../src/lib/TaskTabs.svelte";
  import type { Task } from "../../src/lib/task";

  function task(id: string, title: string, status: Task["status"]): Task {
    return {
      id, runId: status === "running" ? `run-${id}` : null, title, agent: "pi",
      status, projectId: "project-1", projectPath: "F:/demo", sessionId: `session-${id}`,
      sessionFile: null, executionTarget: "local", interactionMode: "rpc",
      piEnvironment: "managed", createdAt: 1, startedAt: 1, completedAt: null, archivedAt: null,
    };
  }
  let tasks = $state<Task[]>([
    task("t1", "主线任务", "running"),
    task("t2", "并行任务", "waiting"),
    task("t3", "已完成任务", "completed"),
  ]);
  let activeTaskId = $state<string | null>("t1");
  let splitActive = $state(false);
  let action = $state("");

  const noop = () => {};
</script>

<nav aria-label="测试状态">
  <output aria-label="操作结果">{action}</output>
  <output aria-label="分栏状态">{splitActive ? "分栏" : "单栏"}</output>
</nav>

<div style="padding: 12px;">
  <TaskTabs
    {tasks}
    {activeTaskId}
    {splitActive}
    onOpen={(t) => { action = `打开 ${t.title}`; }}
    onRename={(t) => { action = `重命名 ${t.title}`; }}
    onStop={noop}
    onArchive={noop}
    onRestart={noop}
    onRestore={noop}
    onDelete={noop}
    onClose={noop}
    onAdd={noop}
    onSplit={(t, direction) => { splitActive = true; action = `分栏 ${t.title} ${direction}`; }}
    onUnsplit={() => { splitActive = false; action = "取消分栏"; }}
  />
</div>
