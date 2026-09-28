<script lang="ts">
  import GitDiffView from "../../src/lib/GitDiffView.svelte";
  import type { GitDiff, GitDiffSelection, ConflictSide } from "../../src/lib/git-diff";

  let selection = $state<GitDiffSelection>({ projectId: "fixture", path: "src/中文 file.ts", area: "unstaged" });
  let closed = $state(false);
  let requests = $state(0);
  const text = 'diff --git a/src/file.ts b/src/file.ts\n--- a/src/file.ts\n+++ b/src/file.ts\n@@ -1,2 +1,3 @@\n const first = 1;\n-const old = true;\n+const updated = true;\n+<script>alert("not HTML")<\/script>\n';
  const long = `diff --git a/long.txt b/long.txt\n--- /dev/null\n+++ b/long.txt\n@@ -0,0 +1,10000 @@\n${Array.from({ length: 10000 }, (_, index) => `+line ${index + 1}`).join("\n")}\n`;
  function choose(path: string, area: GitDiffSelection["area"] = "unstaged") {
    closed = false;
    selection = { projectId: "fixture", path, area };
  }
  async function readDiff(request: GitDiffSelection & { conflictSide: ConflictSide }): Promise<GitDiff> {
    requests++;
    if (request.path === "slow.txt") await new Promise((resolve) => setTimeout(resolve, 1500));
    if (request.path === "error.txt") throw new Error("Fixture: file status changed");
    return {
      path: request.path, area: request.area, format: request.path === "binary.dat" ? "binary" : "text",
      patch: request.path === "long.txt" ? long : request.area === "conflict"
        ? `@@ -1 +1 @@\n-${request.conflictSide}\n+working tree\n`
        : request.path === "empty.txt" ? "diff --git a/empty.txt b/empty.txt\nnew file mode 100644\n"
        : request.path === "slow.txt" || request.path === "fast.txt" ? `@@ -1 +1 @@\n-old\n+${request.path}\n` : text,
      truncated: request.path === "long.txt", sourceOutsideProject: false,
    };
  }
</script>

<div class="fixture">
  <nav aria-label="测试场景">
    <button onclick={() => choose("src/中文 file.ts")}>文本</button>
    <button onclick={() => choose("long.txt")}>长差异</button>
    <button onclick={() => choose("binary.dat")}>二进制</button>
    <button onclick={() => choose("conflict.txt", "conflict")}>冲突</button>
    <button onclick={() => choose("empty.txt", "untracked")}>空新文件</button>
    <button onclick={() => choose("slow.txt")}>慢请求</button>
    <button onclick={() => choose("fast.txt")}>快请求</button>
    <button onclick={() => choose("error.txt")}>错误</button>
    <output aria-label="读取次数">{requests}</output>
  </nav>
  <main>
    {#if !closed}
      <GitDiffView {selection} refreshToken={0} {readDiff} cancelRead={async () => {}} onClose={() => { closed = true; }} />
    {:else}<p role="status">差异已关闭</p>{/if}
  </main>
</div>

<style>
  :global(html), :global(body), :global(#app) { height: 100%; margin: 0; }
  .fixture { height: 100%; display: flex; flex-direction: column; }
  nav { display: flex; flex-wrap: wrap; gap: 6px; padding: 8px; background: var(--surface); }
  nav button { color: var(--text); background: var(--surface-alt); border: 1px solid var(--border); border-radius: 3px; padding: 4px 8px; }
  output { color: var(--text-muted); font-size: 12px; }
  main { position: relative; flex: 1; min-height: 0; min-width: 0; }
</style>
