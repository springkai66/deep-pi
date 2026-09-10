<script lang="ts">
  import GitCommitPanel from "../../src/lib/GitCommitPanel.svelte";
  import type { CommitPorts } from "../../src/lib/git-commit";

  let projectId = $state("first");
  let refreshToken = $state(0);
  let mode = $state("success");
  let preparations = $state(0);
  let commits = $state(0);
  let applied = $state(0);
  const ports: CommitPorts = {
    async prepare(id) {
      preparations++;
      await new Promise((resolve) => setTimeout(resolve, 800));
      return { reference: "refs/heads/main", head: null, tree: "a".repeat(40),
        author: "Fixture <fixture@localhost>", committer: "Fixture <fixture@localhost>", sign: false,
        paths: [`${id}/中文目录/very-long-selected-file-name.ts`, `${id}/deleted.txt`] };
    },
    async commit() {
      commits++;
      const outcome = mode;
      await new Promise((resolve) => setTimeout(resolve, 1200));
      if (outcome === "cancel") return null;
      if (outcome === "error") throw new Error("Fixture: staged tree changed");
      if (outcome === "unknown") return { outcome: "unknown", oid: "b".repeat(40), detail: "Fixture: verify reference before retry" };
      return { outcome: "committed", oid: "b".repeat(40), detail: "" };
    },
  };
</script>

<nav aria-label="测试场景">
  <label>提交结果<select bind:value={mode}><option value="success">成功</option><option value="cancel">取消确认</option><option value="unknown">结果未知</option><option value="error">失败</option></select></label>
  <button onclick={() => { projectId = projectId === "first" ? "second" : "first"; }}>切换项目</button>
  <button onclick={() => { refreshToken++; }}>外部变更</button>
  <output aria-label="当前项目">{projectId}</output>
  <output aria-label="准备次数">{preparations}</output>
  <output aria-label="提交次数">{commits}</output>
  <output aria-label="成功次数">{applied}</output>
</nav>
<main>
  <aside>
    <GitCommitPanel {projectId} {refreshToken} visible={true} blocked={false} stagedCount={2} {ports}
      onBusyChange={() => {}} onCommitted={() => { applied++; }} />
  </aside>
</main>

<style>
  :global(html), :global(body), :global(#app) { height: 100%; margin: 0; }
  :global(#app) { display: flex; flex-direction: column; }
  nav { display: flex; flex-wrap: wrap; gap: 8px; padding: 8px; color: var(--text); font-size: 12px; }
  label { display: flex; align-items: center; gap: 4px; }
  button, select { background: var(--surface); color: var(--text); border: 1px solid var(--border); border-radius: 3px; }
  main { flex: 1; min-height: 0; display: flex; justify-content: flex-end; }
  aside { display: flex; flex-direction: column; width: min(300px, 100%); min-width: 0; background: var(--surface); border-left: 1px solid var(--border); }
</style>
