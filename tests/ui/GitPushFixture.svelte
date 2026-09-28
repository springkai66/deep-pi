<script lang="ts">
  import GitSidebar from "../../src/lib/GitSidebar.svelte";
  import type { PushPorts, PushResult } from "../../src/lib/git-push";
  import type { GitStatus } from "../../src/lib/git-status";

  let mode = $state("success");
  let verificationMode = $state("matches");
  let syncMode = $state("success");
  let syncs = $state(0);
  let synced = $state(false);
  let pushes = $state(0);
  let cancellations = $state(0);
  let projectId = $state("first");
  let busy = $state(false);
  let resolvePush: ((result: PushResult | null) => void) | null = null;
  let pending: PushResult | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  const pushPorts: PushPorts = {
    async load() {
      return { sourceRef: "refs/heads/main", sourceOid: "a".repeat(40), targets: [
        { remote: "origin", destination: "https://github.com/example/long-project-name-for-layout-check.git" },
        { remote: "backup", destination: "git@github.com:example/backup.git" },
      ] };
    },
    async push(_id, expected) {
      pushes++;
      const outcome = mode;
      pending = { outcome: outcome === "rejected" ? "rejected" : "pushed", sourceOid: expected.sourceOid,
        targetRef: expected.targetRef, detail: outcome === "rejected" ? "远程拒绝推送；未强推。" : "远程已接受所选提交。" };
      return new Promise((resolve) => {
        resolvePush = resolve;
        timer = setTimeout(() => { resolve(outcome === "cancel" ? null : pending); resolvePush = null; }, outcome === "slow" ? 10000 : 1200);
      });
    },
    async cancel() {
      cancellations++;
      if (timer) clearTimeout(timer);
      if (resolvePush && pending) resolvePush({ ...pending, outcome: "unknown", detail: "已请求取消，请核对远程结果后再操作。" });
      resolvePush = null;
    },
    async verify(_id, expected) {
      if (verificationMode === "error") throw new Error("Fixture: network unavailable");
      const outcome = verificationMode === "missing" ? "missing" : verificationMode === "different" ? "different" : "matches";
      return { outcome, sourceOid: expected.sourceOid, targetRef: expected.targetRef,
        remoteOid: outcome === "missing" ? null : outcome === "matches" ? expected.sourceOid : "b".repeat(40) };
    },
    async sync(_id, expected) {
      syncs++;
      if (syncMode === "cancel") return null;
      if (syncMode === "error") throw new Error("Fixture: tracking mapping changed");
      synced = syncMode !== "unknown";
      return { outcome: syncMode === "unknown" ? "unknown" : "synced", oid: expected.sourceOid,
        references: ["refs/remotes/origin/main"] };
    },
  };
  async function readStatus(): Promise<GitStatus> {
    return { branch: "main", upstream: "origin/main", oid: "a".repeat(40), unborn: false, detached: false,
      ahead: synced ? 0 : 2, behind: 0, repositoryRoot: "C:/fixture", projectPrefix: "", entries: [
        { path: "src/中文目录/selected.ts", originalPath: null, sourceOutsideProject: false, kind: "tracked", indexStatus: "M", worktreeStatus: "." },
      ] };
  }
</script>

<nav aria-label="测试场景">
  <label>推送结果<select bind:value={mode}><option value="success">成功</option><option value="rejected">拒绝</option><option value="cancel">取消确认</option><option value="slow">慢请求</option></select></label>
  <label>核对结果<select bind:value={verificationMode}><option value="matches">一致</option><option value="different">不同</option><option value="missing">不存在</option><option value="error">查询失败</option></select></label>
  <label>同步结果<select bind:value={syncMode}><option value="success">成功</option><option value="unknown">未知</option><option value="cancel">取消确认</option><option value="error">失败</option></select></label>
  <button onclick={() => { projectId = projectId === "first" ? "second" : "first"; }}>切换项目</button>
  <output aria-label="推送次数">{pushes}</output>
  <output aria-label="同步次数">{syncs}</output>
  <output aria-label="取消次数">{cancellations}</output>
  <output aria-label="写入状态">{busy ? "处理中" : "空闲"}</output>
</nav>
<main>
  <GitSidebar {projectId} visible={true} refreshToken={0} selected={null} available={true}
    {readStatus} {pushPorts} cancelRead={async () => {}} writeIndex={async () => false}
    commitPorts={{ prepare: async () => ({ reference: "refs/heads/main", head: null, tree: "a".repeat(40), paths: ["src/中文目录/selected.ts"], author: "Test <test@localhost>", committer: "Test <test@localhost>", sign: false }), commit: async () => null }}
    onChanged={() => {}} onOpen={() => {}} onClose={() => {}} onBusyChange={(next) => { busy = next; }} />
</main>

<style>
  :global(html), :global(body), :global(#app) { height: 100%; margin: 0; }
  :global(#app) { display: flex; flex-direction: column; }
  nav { display: flex; flex-wrap: wrap; gap: 8px; padding: 8px; color: var(--text); font-size: 12px; }
  label { display: flex; align-items: center; gap: 4px; }
  button, select { background: var(--surface); color: var(--text); border: 1px solid var(--border); border-radius: 3px; }
  main { flex: 1; min-height: 0; display: flex; justify-content: flex-end; }
  main :global(.git-sidebar) { width: 300px; }
</style>
