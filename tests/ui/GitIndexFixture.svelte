<script lang="ts">
  import GitSidebar from "../../src/lib/GitSidebar.svelte";
  import type { GitEntry, GitStatus } from "../../src/lib/git-status";
  import type { IndexRequest } from "../../src/lib/git-index";

  const initial: GitEntry[] = [
    { path: "src/非常长的中文目录/selected-file-with-a-long-name.ts", originalPath: null, sourceOutsideProject: false, kind: "tracked", indexStatus: ".", worktreeStatus: "M" },
    { path: "staged.txt", originalPath: null, sourceOutsideProject: false, kind: "tracked", indexStatus: "M", worktreeStatus: "." },
    { path: "conflict.txt", originalPath: null, sourceOutsideProject: false, kind: "conflict", indexStatus: "U", worktreeStatus: "U" },
    { path: "cross.txt", originalPath: null, sourceOutsideProject: true, kind: "tracked", indexStatus: "R", worktreeStatus: "." },
  ];
  let entries = initial.map((entry) => ({ ...entry }));
  let writes = $state(0);
  let applied = $state(0);
  let mode = $state("success");
  let projectId = $state("fixture");
  let selected = $state("");
  async function readStatus(): Promise<GitStatus> {
    return { branch: "main", upstream: "origin/main", oid: "abc", unborn: false, detached: false,
      ahead: 1, behind: 0, repositoryRoot: "C:/fixture", projectPrefix: "", entries: entries.map((entry) => ({ ...entry })) };
  }
  async function writeIndex(_id: string, request: IndexRequest) {
    writes++;
    const outcome = mode;
    await new Promise((resolve) => setTimeout(resolve, 1200));
    if (outcome === "error") throw new Error("Fixture: index locked");
    if (outcome === "cancel") return false;
    entries = entries.map((entry) => entry.path !== request.entry.path ? entry : {
      ...entry, kind: "tracked",
      indexStatus: request.action === "unstage" ? "." : "M",
      worktreeStatus: request.action === "unstage" ? "M" : ".",
    });
    return true;
  }
</script>

<nav aria-label="测试场景">
  <label>写入结果<select bind:value={mode}><option value="success">成功</option><option value="error">失败</option><option value="cancel">取消确认</option></select></label>
  <button onclick={() => { projectId = projectId === "fixture" ? "another" : "fixture"; }}>切换项目</button>
  <output aria-label="写入次数">{writes}</output>
  <output aria-label="刷新次数">{applied}</output>
  <output aria-label="打开差异">{selected}</output>
</nav>
<main>
  <GitSidebar {projectId} visible={true} refreshToken={0} selected={null} available={true}
    {readStatus} {writeIndex} cancelRead={async () => {}}
    onBusyChange={() => {}} onChanged={() => { applied++; }} onOpen={(path) => { selected = path; }} onClose={() => {}} />
</main>

<style>
  :global(html), :global(body), :global(#app) { height: 100%; margin: 0; }
  :global(#app) { display: flex; flex-direction: column; }
  nav { display: flex; flex-wrap: wrap; gap: 8px; padding: 8px; color: var(--text); }
  label { display: flex; align-items: center; gap: 4px; }
  button, select { background: var(--surface); color: var(--text); border: 1px solid var(--border); border-radius: 3px; }
  main { flex: 1; min-height: 0; display: flex; justify-content: flex-end; }
  main :global(.git-sidebar) { width: 300px; }
</style>
