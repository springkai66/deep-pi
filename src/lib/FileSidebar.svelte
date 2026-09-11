<script lang="ts">
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { ChevronDown, ChevronRight, File, Folder, RefreshCw, Search, Square, X, Radio } from "@lucide/svelte";
  import { onDestroy, onMount, untrack } from "svelte";
  import type { Project } from "./project";
  import { visibleFiles, type FileEntry, type FileIndex } from "./files";
  import { createLatestSearch, type SearchMatch } from "./search";
  import ContentSearchResults from "./ContentSearchResults.svelte";
  import { shortcutAria } from "./shortcuts";

  interface Props {
    project: Project | undefined;
    onOpen: (path: string, line?: number, column?: number) => void;
    searchFocusToken: number;
    visible: boolean;
    refreshToken: number;
    watchError?: string;
    onRetryWatch?: () => void;
  }
  let { project, onOpen, searchFocusToken, visible, refreshToken, watchError = "", onRetryWatch }: Props = $props();
  let entries = $state<FileEntry[]>([]);
  let expanded = $state<Set<string>>(new Set());
  let loaded = new Set<string>();
  // 已确认没有任何子项的目录：用于在树里提示「空目录」，否则展开空目录与
  // 「展开失败」在视觉上无法区分。用数组而非 Set——Svelte 5 会代理数组，
  // 但不会代理 Set，`Set.has` 在模板里不会建立依赖。
  let emptyDirectories = $state<string[]>([]);
  let pending = $state<Set<string>>(new Set());
  let loading = $state(false);
  let error = $state("");
  let incomplete = $state(false);
  let unreadable = $state<string[]>([]);
  let searchText = $state("");
  let query = $state("");
  let activePath = $state("");
  let searchInput = $state<HTMLInputElement>();
  let list = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let height = $state(400);
  let generation = 0;
  let directoryQueue = Promise.resolve();
  let composing = $state(false);
  let searchMode = $state<"files" | "content">("files");
  let contentResults = $state<SearchMatch[]>([]);
  let contentTruncated = $state(false);
  let contentLoading = $state(false);
  let contentError = $state("");
  let contentSkipped = $state(0);
  let contentFocusToken = $state(0);
  let contentRefreshToken = $state(0);
  let contentRefreshPending = $state(false);
  let contentRerun = $state(0);
  let lastContentKey = "";
  type ContentResult = { matches: SearchMatch[]; truncated: boolean; skippedFiles: number };
  const searchRequests = new Map<string, { projectId: string; query: string }>();
  const contentSearch = createLatestSearch<ContentResult>(
    (id) => {
      const request = searchRequests.get(id)!;
      return invoke("search_project_files", {
        projectId: request.projectId, operationId: id,
        request: { query: request.query, relativePath: null, caseSensitive: false },
      });
    },
    (operationId) => invoke("cancel_project_search", { operationId }),
  );
  let debounce: ReturnType<typeof setTimeout> | undefined;
  let refreshTimer: ReturnType<typeof setTimeout> | undefined;
  let refreshAgain = false;
  const rowHeight = 28;
  const rows = $derived(visibleFiles(entries, query, expanded));
  const start = $derived(Math.max(0, Math.min(Math.floor(scrollTop / rowHeight) - 8, rows.length - 1)));
  const end = $derived(Math.min(rows.length, start + Math.ceil(height / rowHeight) + 16));
  const windowRows = $derived(rows.slice(start, end));

  $effect(() => {
    const selected = project;
    untrack(() => { void refresh(selected, true); });
  });
  function scheduleRefresh() {
    clearTimeout(refreshTimer);
    refreshTimer = setTimeout(() => {
      if (!visible) return;
      if (loading) refreshAgain = true;
      else void refresh();
    }, 250);
  }
  $effect(() => {
    if (visible && refreshToken > 0) untrack(scheduleRefresh);
  });
  onMount(() => {
    const focus = () => { if (visible) scheduleRefresh(); };
    window.addEventListener("focus", focus);
    return () => window.removeEventListener("focus", focus);
  });
  $effect(() => {
    void contentRefreshToken;
    untrack(() => { contentRefreshPending = true; });
  });
  $effect(() => {
    if (contentRefreshPending && !contentLoading && !composing) {
      untrack(() => { contentRefreshPending = false; contentRerun++; });
    }
  });
  $effect(() => {
    const value = searchText;
    const projectId = project?.id;
    void contentRerun;
    const key = `${projectId}:${searchMode}:${value}`;
    contentSearch.cancel();
    if (key !== lastContentKey) contentResults = [];
    lastContentKey = key;
    contentTruncated = false;
    contentSkipped = 0;
    contentLoading = false;
    contentError = "";
    if (searchMode !== "content" || composing || !projectId || !value.trim() || !isTauri()) return;
    const id = crypto.randomUUID();
    searchRequests.set(id, { projectId, query: value });
    const timer = setTimeout(() => {
      contentLoading = true;
      void contentSearch.run(id, (result) => {
        contentResults = result.matches;
        contentTruncated = result.truncated;
        contentSkipped = result.skippedFiles;
        contentLoading = false;
      }, (cause) => {
        contentError = String(cause);
        contentLoading = false;
      }).finally(() => searchRequests.delete(id));
    }, 180);
    return () => { clearTimeout(timer); contentSearch.cancel(); searchRequests.delete(id); };
  });
  $effect(() => {
    if (searchFocusToken > 0) { searchMode = "files"; searchInput?.focus(); searchInput?.select(); }
  });
  $effect(() => {
    void query;
    if (list) list.scrollTop = 0;
    scrollTop = 0;
    activePath = "";
  });
  onDestroy(() => { generation++; clearTimeout(debounce); clearTimeout(refreshTimer); contentSearch.cancel(); });

  async function refresh(selected = project, reset = false) {
    const current = ++generation;
    pending = new Set();
    loaded = new Set();
    emptyDirectories = [];
    error = "";
    if (reset) {
      refreshAgain = false;
      entries = [];
      expanded = new Set();
      searchText = query = "";
      clearTimeout(debounce);
    }
    incomplete = false;
    unreadable = [];
    if (!selected || !isTauri()) { loading = false; return; }
    loading = true;
    try {
      const result = await invoke<FileIndex>("list_project_files", {
        projectId: selected.id, relativePath: "", recursive: true,
      });
      if (current !== generation) return;
      entries = result.entries;
      incomplete = result.truncated;
      unreadable = result.unreadableDirectories;
      contentRefreshToken++;
    } catch (cause) {
      if (current === generation) error = String(cause);
    } finally {
      if (current === generation) {
        loading = false;
        if (refreshAgain) { refreshAgain = false; scheduleRefresh(); }
      }
    }
  }

  async function toggle(entry: FileEntry) {
    if (!entry.isDirectory) { onOpen(entry.path); return; }
    if (query.trim()) return;
    const next = new Set(expanded);
    if (next.has(entry.path)) { next.delete(entry.path); expanded = next; return; }
    next.add(entry.path);
    expanded = next;
    if (loaded.has(entry.path) || pending.has(entry.path) || !project || loading) return;
    const current = generation;
    const projectId = project.id;
    pending = new Set(pending).add(entry.path);
    try {
      const request = directoryQueue.then(async () => {
        if (current !== generation) return null;
        return invoke<FileIndex>("list_project_files", {
          projectId, relativePath: entry.path, recursive: false,
        });
      });
      directoryQueue = request.then(() => {}, () => {});
      const result = await request;
      if (current !== generation) return;
      if (!result) return;
      const directChildren = new Set(result.entries.map((child) => child.path));
      const prefix = `${entry.path}/`;
      const retained = entries.filter((child) => {
        if (!child.path.startsWith(prefix)) return true;
        const immediateChild = prefix + child.path.slice(prefix.length).split("/")[0];
        return directChildren.has(immediateChild);
      });
      const merged = new Map(retained.map((child) => [child.path, child]));
      for (const child of result.entries) merged.set(child.path, child);
      entries = [...merged.values()];
      loaded.add(entry.path);
      // 仅当目录内确实没有任何条目时标记为空；被忽略规则排除的内容也会留 unreadable 提示，
      // 不会因此误报为空目录。
      emptyDirectories = result.entries.length === 0
        ? [...new Set([...emptyDirectories, entry.path])]
        : emptyDirectories.filter((path) => path !== entry.path);
      incomplete ||= result.truncated;
      error = "";
    } catch (cause) {
      if (current === generation) {
        error = String(cause);
        const retryable = new Set(expanded);
        retryable.delete(entry.path);
        expanded = retryable;
      }
    } finally {
      if (current === generation) {
        const nextPending = new Set(pending);
        nextPending.delete(entry.path);
        pending = nextPending;
      }
    }
  }

  function commitQuery(value: string) {
    clearTimeout(debounce);
    debounce = setTimeout(() => { query = value; }, 80);
  }

  function clearSearch() {
    clearTimeout(debounce);
    searchText = query = "";
    searchInput?.focus();
  }

  function keyboard(event: KeyboardEvent) {
    if (event.isComposing || composing) return;
    if (event.key === "Escape") { event.preventDefault(); clearSearch(); return; }
    if (searchMode === "content") {
      if (event.key === "ArrowDown" && contentResults.length) { event.preventDefault(); contentFocusToken++; }
      return;
    }
    const index = rows.findIndex((entry) => entry.path === activePath);
    if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      if (event.target === searchInput && ["Home", "End"].includes(event.key)) return;
      event.preventDefault();
      const next = event.key === "Home" ? 0 : event.key === "End" ? rows.length - 1
        : index < 0 ? (event.key === "ArrowDown" ? 0 : rows.length - 1)
        : Math.max(0, Math.min(rows.length - 1, index + (event.key === "ArrowDown" ? 1 : -1)));
      if (!rows[next]) return;
      activePath = rows[next].path;
      if (list) {
        if (next * rowHeight < list.scrollTop) list.scrollTop = next * rowHeight;
        else if ((next + 1) * rowHeight > list.scrollTop + height) list.scrollTop = (next + 1) * rowHeight - height;
        list.focus();
      }
    } else if (event.key === "Enter") {
      const entry = rows[index] ?? rows.find((row) => !row.isDirectory);
      if (entry) { event.preventDefault(); void toggle(entry); }
    } else if (event.target !== searchInput && !query.trim() && (event.key === "ArrowRight" || event.key === "ArrowLeft")) {
      const entry = rows[index];
      if (entry?.isDirectory && expanded.has(entry.path) !== (event.key === "ArrowRight")) {
        event.preventDefault();
        void toggle(entry);
      }
    }
  }
  function rowId(path: string) { return `file-${encodeURIComponent(path)}`; }
</script>

<section class="file-sidebar" aria-label="项目文件">
  <header>
    <strong title={project?.path}>{project?.name ?? "未选择项目"}</strong>
    {#if watchError && onRetryWatch}
      <button type="button" title="重新连接文件监听" aria-label="重新连接文件监听" onclick={onRetryWatch}><Radio size={15} /></button>
    {/if}
    <button type="button" disabled={loading || !project} title="刷新文件" aria-label="刷新文件" onclick={() => void refresh()}>
      <RefreshCw size={15} />
    </button>
  </header>
  {#if watchError}<p role="status" class="error">{watchError}</p>{/if}
  <div class="file-search" role="search">
    <Search size={14} aria-hidden="true" />
    <input type="search" bind:this={searchInput} bind:value={searchText} aria-label="搜索文件" aria-keyshortcuts={shortcutAria("files")}
      placeholder={searchMode === "files" ? "搜索文件名或路径" : "搜索文件内容"} onkeydown={keyboard}
      oncompositionstart={() => { composing = true; clearTimeout(debounce); }}
      oncompositionend={(event) => { composing = false; commitQuery(event.currentTarget.value); }}
      oninput={(event) => { if (!composing) commitQuery(event.currentTarget.value); }} />
    {#if searchText}<button type="button" title="清空文件搜索" aria-label="清空文件搜索" onclick={clearSearch}><X size={14} /></button>{/if}
  </div>
  <div class="search-modes" role="group" aria-label="搜索类型">
    <button type="button" aria-pressed={searchMode === "files"} onclick={() => { searchMode = "files"; }}>文件</button>
    <button type="button" aria-pressed={searchMode === "content"} onclick={() => { searchMode = "content"; }}>内容</button>
  </div>
  {#if searchMode === "content"}
    {#if contentLoading}
      <div class="search-progress"><p role="status">正在搜索内容…</p>
        <button type="button" title="取消内容搜索" aria-label="取消内容搜索" onclick={() => { contentRefreshPending = false; contentSearch.cancel(); contentLoading = false; contentError = "搜索已取消"; }}><Square size={14} /></button>
      </div>
    {/if}
    {#if contentError}<p role="alert" class="error">{contentError}</p>{/if}
    {#if contentTruncated}<p role="status">搜索结果已截断，请缩小查询范围。</p>{/if}
    {#if contentSkipped}<p role="status">{contentSkipped} 个文件不可读、不支持或超过 2 MiB</p>{/if}
    {#if !project}<p role="status">未选择项目</p>
    {:else if !contentLoading && !contentError && !contentResults.length && searchText.trim()}<p role="status">没有匹配内容</p>{/if}
    <ContentSearchResults matches={contentResults} {onOpen} onClear={clearSearch} focusToken={contentFocusToken} />
  {:else}
  {#if loading}<p role="status">正在读取项目文件…</p>{/if}
  {#if error}<p role="alert" class="error">{error}</p>{/if}
  {#if incomplete}<p role="status">索引未完整加载；展开目录可继续读取。</p>{/if}
  {#if unreadable.length}<p role="status" title={unreadable.join("\n")}>{unreadable.length} 个目录无法读取</p>{/if}
  {#if !loading && rows.length === 0}<p role="status">{query ? "没有匹配的文件" : project ? "目录为空" : "未选择项目"}</p>{/if}
  <div class="file-tree" role="tree" aria-label="文件目录" tabindex="0"
    aria-activedescendant={windowRows.some((entry) => entry.path === activePath) ? rowId(activePath) : undefined}
    aria-busy={loading} bind:this={list} bind:clientHeight={height}
    onscroll={(event) => { scrollTop = event.currentTarget.scrollTop; }} onkeydown={keyboard}>
    <div role="none" style:height={`${start * rowHeight}px`}></div>
    {#each windowRows as entry (entry.path)}
      <button type="button" role="treeitem" id={rowId(entry.path)} tabindex="-1" class:active={activePath === entry.path}
        aria-level={entry.path.split("/").length}
        aria-selected={activePath === entry.path}
        aria-expanded={entry.isDirectory ? !!query.trim() || expanded.has(entry.path) : undefined}
        style:padding-left={`${8 + (entry.path.split("/").length - 1) * 12}px`}
        title={entry.path} onclick={() => { activePath = entry.path; void toggle(entry); }}>
        {#if entry.isDirectory}
          {#if query.trim() || expanded.has(entry.path)}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if}
          <Folder size={14} />
        {:else}<span class="indent"></span><File size={14} />{/if}
        <span class="filename">{entry.name}</span>
        {#if pending.has(entry.path)}<span aria-label="正在加载">…</span>{/if}
      </button>
      {#if entry.isDirectory && !query.trim() && expanded.has(entry.path) && emptyDirectories.includes(entry.path)}
        <p class="empty-directory" role="status" style:padding-left={`${8 + (entry.path.split("/").length - 1) * 12 + 12}px`}>空目录</p>
      {/if}
    {/each}
    <div role="none" style:height={`${Math.max(0, rows.length - end) * rowHeight}px`}></div>
  </div>
  {/if}
  <footer>{entries.filter((entry) => !entry.isDirectory).length} 个文件</footer>
</section>

<style>
  .file-sidebar { display: flex; flex-direction: column; min-height: 0; height: 100%; }
  header { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 8px 10px; }
  header strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; }
  header button, .file-search button { display: grid; place-items: center; flex-shrink: 0; width: 24px; height: 24px; border: 0; border-radius: 4px; background: transparent; color: var(--text-muted); cursor: pointer; }
  button:disabled { opacity: .4; cursor: default; }
  .file-search { display: flex; align-items: center; gap: 6px; margin: 0 10px 8px; padding: 4px 6px; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface); color: var(--text-muted); }
  .file-search input { width: 100%; min-width: 0; border: 0; background: transparent; color: var(--text); }
  .search-modes { display: flex; gap: 2px; margin: 0 10px 6px; }
  .search-modes button { flex: 1; border: 0; border-radius: 3px; padding: 4px; background: transparent; color: var(--text-muted); font-size: 11px; cursor: pointer; }
  .search-modes button[aria-pressed="true"] { background: var(--surface-hover); color: var(--text); }
  .search-progress { display: flex; align-items: center; justify-content: space-between; padding-right: 10px; }
  .search-progress button { width: 26px; height: 26px; display: grid; place-items: center; border: 0; color: var(--text); background: var(--surface); cursor: pointer; }
  p { margin: 6px 10px; color: var(--text-muted); font-size: 12px; overflow-wrap: anywhere; }
  /* 空目录占位行：与文件行同高，不参与文件计数。 */
  .file-tree p.empty-directory { margin: 0; height: 28px; display: flex; align-items: center; color: var(--text-muted); font-size: 11px; font-style: italic; }
  .error { color: #ce5147; }
  .file-tree { flex: 1; min-height: 0; overflow: auto; }
  .file-tree button { display: flex; align-items: center; gap: 5px; width: 100%; height: 28px; padding-right: 8px; border: 0; color: var(--text); background: transparent; text-align: left; cursor: pointer; }
  .file-tree button:hover, .file-tree button.active { background: var(--surface-hover); }
  .file-tree :global(svg), .indent { flex-shrink: 0; }
  .filename { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .indent { width: 12px; }
  footer { padding: 8px 10px; border-top: 1px solid var(--border); font-size: 11px; color: var(--text-muted); }
</style>
