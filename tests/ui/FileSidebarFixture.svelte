<script lang="ts">
  import FileSidebar from "../../src/lib/FileSidebar.svelte";
  import type { Project } from "../../src/lib/project";

  let project = $state<Project>({ id: "first", name: "First", path: "C:/first", createdAt: 0, lastOpenedAt: 0 });
  let refreshToken = $state(0);
  let lastAction = $state("");
</script>

<div class="controls">
  <button id="watch-change" onclick={() => { refreshToken++; }}>Watch change</button>
  <button id="focus-change" onclick={() => window.dispatchEvent(new Event("focus"))}>Window focus</button>
  <button id="replace-project" onclick={() => { project = { ...project, name: "First renamed" }; }}>Update project metadata</button>
  <button id="switch-project" onclick={() => { project = { id: "second", name: "Second", path: "C:/second", createdAt: 0, lastOpenedAt: 0 }; }}>Switch project</button>
</div>
<main>
  <FileSidebar {project} {refreshToken} visible={true} searchFocusToken={0} onOpen={() => {}}
    onAction={async (action, entry) => { lastAction = `${action}:${entry?.path ?? "root"}`; }} />
  <output id="last-action">{lastAction}</output>
</main>

<style>
  .controls { display: flex; gap: 8px; }
  main { height: 230px; width: 310px; }
</style>
