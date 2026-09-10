<script lang="ts">
  import FileEditor from "../../src/lib/FileEditor.svelte";
  import type { invoke } from "@tauri-apps/api/core";
  import AppDialog from "../../src/lib/AppDialog.svelte";
  import { createDialogQueue, type DialogRequest } from "../../src/lib/dialog";
  import { createFileWorkspace, type FileDocument, type FileSaveResult } from "../../src/lib/file-workspace";
  let documents = $state.raw<FileDocument[]>([]);
  let path = $state("src/中文示例.ts");
  let visible = $state(true);
  let saves = $state(0);
  let refreshToken = $state(0);
  let mode = $state("success");
  let request = $state<DialogRequest | null>(null);
  let pendingSave: (() => void) | null = null;
  const dialog = createDialogQueue((next) => { request = next; });
  const disk = new Map([
    ["src/中文示例.ts", { content: "\uFEFFconst greeting = \"你好，世界\";\r\nconsole.log(greeting);\r\n", version: "v1" }],
    ["README.md", { content: "# DeepPi\n\n第二个文件。\n", version: "v1" }],
  ]);
  const controller = createFileWorkspace({
    read: async (_project, relative) => {
      const item = disk.get(relative);
      if (!item) throw new Error("文件不存在");
      return { path: relative, ...item, size: new TextEncoder().encode(item.content).length };
    },
    save: async (_project, relative, content, version): Promise<FileSaveResult> => {
      saves++;
      if (mode === "delayed") await new Promise<void>((resolve) => { pendingSave = resolve; });
      if (mode === "failure") throw new Error("IPC interrupted");
      const current = disk.get(relative);
      if (version === null && current || version !== null && version !== current?.version) {
        return { outcome: "conflict", version: null, recoveryPath: null, pendingPath: null, detail: "文件已被外部修改，未写入。" };
      }
      const next = `v${saves + 1}`;
      disk.set(relative, { content, version: next });
      return { outcome: "saved", version: next, recoveryPath: null, pendingPath: null, detail: "已保存。" };
    },
    changed: (next) => { documents = next; },
    saved: () => { refreshToken++; },
    chooseClose: async () => {
      const result = await dialog.request({ kind: "choice", title: "未保存的文件", message: path,
        choices: [{ value: "save", label: "保存后继续" }, { value: "discard", label: "放弃未保存的修改" }] });
      return result === "save" || result === "discard" ? result : null;
    },
  });
  const invokeCommand: typeof invoke = async <T,>(command: string, args?: Parameters<typeof invoke>[1]): Promise<T> => {
    if (command === "read_project_file" && args && !Array.isArray(args) && !(args instanceof ArrayBuffer)) {
      const relative = String((args as Record<string, unknown>).relativePath);
      const item = disk.get(relative);
      if (!item) throw new Error("文件不存在");
      return { path: relative, ...item, size: new TextEncoder().encode(item.content).length } as T;
    }
    throw new Error(`Unsupported fixture command: ${command}`);
  };
  function open(relative: string) { path = relative; visible = true; void controller.open("fixture", relative); }
  open("src/中文示例.ts");
</script>
<div class="fixture-controls">
  <button onclick={() => open("src/中文示例.ts")}>打开代码</button>
  <button onclick={() => open("README.md")}>打开说明</button>
  <button onclick={() => { visible = !visible; }}>切换视图</button>
  <button onclick={() => { disk.set(path, { content: "外部新内容\n", version: "external" }); refreshToken++; }}>外部修改</button>
  <label>保存模式<select bind:value={mode}><option value="success">成功</option><option value="failure">失败</option><option value="delayed">延迟</option></select></label>
  <button onclick={() => { pendingSave?.(); pendingSave = null; }}>完成保存</button>
  <button onclick={() => { document.documentElement.dataset.colorScheme = "dark"; }}>深色</button>
  <button onclick={() => { document.documentElement.dataset.colorScheme = "light"; }}>浅色</button>
  <output>保存次数 {saves}</output>
</div>
<main>
  <FileEditor {documents} {controller} {invokeCommand} projectId="fixture" {path} {visible} {refreshToken}
    editorConfigured={false} onConfigureEditor={() => {}} onSelect={open} onHide={() => { visible = false; }}
    requestDialog={(options) => dialog.request(options)} />
</main>
<AppDialog {request} onResolve={(value) => dialog.resolve(value)} />
<style>
  .fixture-controls { display: flex; gap: 8px; flex-wrap: wrap; min-height: 40px; align-items: center; }
  main { position: relative; height: calc(100vh - 80px); min-height: 300px; }
</style>
