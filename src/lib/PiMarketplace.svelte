<script lang="ts">
  import { invoke as nativeInvoke } from "@tauri-apps/api/core";
  import { PackageOpen, RefreshCw, Search, Trash2, X, ArrowUpCircle } from "@lucide/svelte";
  import { onDestroy, onMount } from "svelte";
  import { createOperationRunner, type OperationState } from "$lib/operation";
  import { getLocale, t } from "$lib/i18n.svelte";

  interface PiPackage {
    name: string;
    description: string;
    types: string[];
    downloads: number;
    publishedAt: number;
    path: string;
  }

  interface InstalledPackage {
    source: string;
    autoload: boolean | null;
    environment: "managed" | "native";
  }

  interface PackageMetadata {
    version: string;
    license: string | null;
  }

  interface Props {
    projectPath: string | null;
    confirm: (title: string, message: string, confirmLabel?: string) => Promise<boolean>;
    onClose: () => void;
    onError: (error: unknown) => void;
    embedded?: boolean;
    onBusyChange?: (busy: boolean) => void;
    invokeCommand?: typeof nativeInvoke;
  }

  let { projectPath, confirm, onClose, onError, embedded = false, onBusyChange = () => {}, invokeCommand = nativeInvoke }: Props = $props();
  const invoke = <T,>(command: string, args?: Parameters<typeof nativeInvoke>[1]) => invokeCommand<T>(command, args);
  type SortMode = "downloads" | "publishedAt";
  const SORT_MODES: readonly SortMode[] = ["downloads", "publishedAt"];
  const SORT_LABELS: Record<SortMode, string> = {
    downloads: "按下载量排序",
    publishedAt: "按发布时间排序",
  };
  let query = $state("");
  let sortBy = $state<SortMode>("downloads");
  let packages = $state<PiPackage[]>([]);
  let installed = $state<InstalledPackage[]>([]);
  let scope = $state<"global" | "project">("global");
  let tab = $state<"installed" | "market">("installed");
  let installedLoading = $state(false);
  let isLoading = $state(false);
  let busyPackage = $state<string | null>(null);
  $effect(() => onBusyChange(busyPackage !== null));
  onDestroy(() => onBusyChange(false));
  let statusMessage = $state("");
  let operationState = $state<OperationState | null>(null);
  const operations = createOperationRunner(invoke, (state) => { operationState = state; });
  let searchGeneration = 0;
  let installedGeneration = 0;

  const canUseProjectScope = $derived(projectPath !== null);
  /** 目录展示顺序：下载量、发布时间均按降序（最多 / 最新在前），并列时按名称稳定排序。 */
  const sortedPackages = $derived.by(() => {
    const mode = sortBy;
    return [...packages].sort((a, b) => b[mode] - a[mode] || a.name.localeCompare(b.name));
  });

  onMount(() => {
    void search();
    void refreshInstalled();
  });

  async function search() {
    const generation = ++searchGeneration;
    isLoading = true;
    statusMessage = "";
    try {
      const result = await invoke<PiPackage[]>("search_pi_packages", {
        request: { query: query.trim() },
      });
      if (generation === searchGeneration) packages = result;
    } catch (error) {
      if (generation === searchGeneration) onError(error);
    } finally {
      if (generation === searchGeneration) isLoading = false;
    }
  }

  async function refreshInstalled() {
    const generation = ++installedGeneration;
    installedLoading = true;
    try {
      const result = await invoke<InstalledPackage[]>("list_pi_packages", {
        scope,
        projectPath,
        environment: "managed",
      });
      if (generation === installedGeneration) installed = result;
    } catch (error) {
      if (generation === installedGeneration) onError(error);
    } finally {
      if (generation === installedGeneration) installedLoading = false;
    }
  }

  async function metadataFor(name: string): Promise<PackageMetadata> {
    return invoke<PackageMetadata>("pi_package_metadata", { request: { name } });
  }

  function installedSource(name: string): InstalledPackage | null {
    const matches = installed.filter((pkg) => {
      const source = pkg.source.replace(/^npm:/, "");
      return source === name || source.startsWith(`${name}@`);
    });
    return matches[0] ?? null;
  }

  function formatDownloads(value: number) {
    if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
    if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
    return String(value);
  }

  function formatDate(value: number) {
    if (!value) return t("日期未知");
    return new Intl.DateTimeFormat(getLocale(), { month: "short", day: "numeric" }).format(value);
  }

  async function install(pkg: PiPackage) {
    if (busyPackage) return;
    if (scope === "project" && !projectPath) return;
    busyPackage = pkg.name;
    try {
      const metadata = await metadataFor(pkg.name);
      if (
        !(await confirm(
          t("安装 Pi Package"),
          t("安装 {name}@{version} 并自动启用吗？", { name: pkg.name, version: metadata.version }),
          t("安装"),
        ))
      ) {
        return;
      }
      await operations.run("package_operation", {
          operation: "install",
          spec: `npm:${pkg.name}@${metadata.version}`,
          scope,
          projectPath,
          environment: "managed",
      });
      statusMessage = t("{name} 已安装并启用", { name: pkg.name });
      await refreshInstalled();
    } catch (error) {
      onError(error);
    } finally {
      busyPackage = null;
    }
  }

  async function updateAll() {
    if (busyPackage) return;
    if (installed.length === 0) return;
    busyPackage = "all";
    try {
      if (
        !(await confirm(
          t("更新 Pi Packages"),
          t("更新当前{scope}范围的全部 Package 吗？", { scope: scope === "global" ? t("全局") : t("项目") }),
          t("全部更新"),
        ))
      ) {
        return;
      }
      await operations.run("package_operation", { operation: "updateAll", spec: "", scope, projectPath, environment: "managed" });
      statusMessage = t("全部 Package 已更新");
      await refreshInstalled();
    } catch (error) {
      onError(error);
    } finally {
      busyPackage = null;
    }
  }

  async function update(pkg: InstalledPackage) {
    if (busyPackage) return;
    busyPackage = pkg.source;
    try {
      if (!(await confirm(t("更新 Pi Package"), t("更新 {name} 吗？", { name: pkg.source }), t("更新")))) return;
      await operations.run("package_operation", { operation: "update", spec: pkg.source, scope, projectPath, environment: "managed" });
      statusMessage = t("{name} 已更新", { name: pkg.source });
      await refreshInstalled();
    } catch (error) {
      onError(error);
    } finally {
      busyPackage = null;
    }
  }

  async function remove(pkg: InstalledPackage) {
    if (busyPackage) return;
    busyPackage = pkg.source;
    try {
      if (!(await confirm(t("卸载 Pi Package"), t("卸载 {name} 吗？", { name: pkg.source }), t("卸载")))) return;
      await operations.run("package_operation", { operation: "remove", spec: pkg.source, scope, projectPath, environment: "managed" });
      statusMessage = t("{name} 已卸载", { name: pkg.source });
      await refreshInstalled();
    } catch (error) {
      onError(error);
    } finally {
      busyPackage = null;
    }
  }

  async function cancelOperation() {
    try {
      const accepted = await operations.cancel();
      statusMessage = accepted ? t("正在取消并恢复") : t("操作尚未接受取消，或已进入提交阶段");
    } catch (error) {
      onError(error);
    }
  }
</script>

<section class="market-page" class:embedded aria-label={t("Pi 扩展市场")}>
  {#if !embedded}
    <header class="market-header">
      <button class="icon-action" type="button" aria-label={t("返回工作区")} title={t("返回工作区")} disabled={busyPackage !== null} onclick={onClose}><X size={17} /></button>
      <h1>{t("Pi 扩展市场")}</h1>
    </header>
  {/if}
  <div class="page-controls">
    <div class="scope-switch" role="group" aria-label={t("安装范围")}>
      <button class:active={scope === "global"} aria-pressed={scope === "global"} type="button" disabled={busyPackage !== null}
        onclick={() => { scope = "global"; void refreshInstalled(); }}>{t("全局")}</button>
      <button class:active={scope === "project"} aria-pressed={scope === "project"} type="button" disabled={!canUseProjectScope || busyPackage !== null}
        title={canUseProjectScope ? "" : t("请先选择一个项目")}
        onclick={() => { scope = "project"; void refreshInstalled(); }}>{t("项目")}</button>
    </div>
    <div class="scope-switch tab-switch" role="group" aria-label={t("视图")}>
      <button class:active={tab === "installed"} aria-pressed={tab === "installed"} type="button" onclick={() => { tab = "installed"; void refreshInstalled(); }}>{t("已安装")}</button>
      <button class:active={tab === "market"} aria-pressed={tab === "market"} type="button" onclick={() => { tab = "market"; }}>{t("市场")}</button>
    </div>
  </div>

  {#if tab === "installed"}
    <section class="settings-group">
      <div class="group-header">
        <h3>{t("已安装")} · {t("Pi 扩展")}</h3>
        <div class="header-actions">
          <button class="icon-action" type="button" aria-label={t("全部更新 Pi Package")} title={t("更新 DeepPi 托管扩展")}
            disabled={busyPackage !== null || installed.length === 0} onclick={() => void updateAll()}><ArrowUpCircle size={14} /></button>
          <button class="icon-action" type="button" aria-label={t("刷新已安装 Package")} title={t("刷新")}
            disabled={installedLoading} onclick={() => void refreshInstalled()}><RefreshCw size={14} /></button>
        </div>
      </div>
      {#if installedLoading && installed.length === 0}
        <p class="muted" role="status">{t("正在加载目录…")}</p>
      {:else if installed.length === 0}
        <p class="muted" role="status">{t("暂无已安装 Package")}</p>
      {:else}
        <ul class="entry-list">
          {#each installed as pkg (pkg.source)}
            <li>
              <div class="entry-main">
                <strong>{pkg.source}</strong>
                <small>{t("DeepPi 托管")}</small>
              </div>
              <button class="icon-action" type="button" aria-label={t("更新 {name}", { name: pkg.source })} title={t("更新")}
                disabled={busyPackage !== null} onclick={() => void update(pkg)}><RefreshCw size={14} /></button>
              <button class="danger-action" type="button" aria-label={t("卸载 {name}", { name: pkg.source })} title={t("卸载")}
                disabled={busyPackage !== null} onclick={() => void remove(pkg)}><Trash2 size={14} /></button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {:else}
    <section class="settings-group">
      <div class="group-header">
        <h3>{t("Pi 扩展市场")}</h3>
        <span class="muted">{t("官方目录")}</span>
      </div>
      <div class="market-toolbar">
        <form class="market-search" onsubmit={(event) => { event.preventDefault(); void search(); }}>
          <Search size={14} />
          <input bind:value={query} aria-label={t("搜索 Pi Package")} placeholder={t("搜索 Pi Package")} autocomplete="off" />
          <button class="primary-action compact" type="submit" disabled={isLoading} aria-label={t("搜索")} title={t("搜索")}><Search size={14} />{t("搜索")}</button>
        </form>
        <select class="market-sort" bind:value={sortBy} aria-label={t("排序")}>
          {#each SORT_MODES as mode (mode)}<option value={mode}>{t(SORT_LABELS[mode])}</option>{/each}
        </select>
      </div>
      {#if isLoading && packages.length === 0}
        <p class="muted" role="status">{t("正在加载目录…")}</p>
      {:else if packages.length === 0}
        <p class="muted" role="status">{t("没有匹配的 Package")}</p>
      {:else}
        <ul class="entry-list market-list">
          {#each sortedPackages as pkg (pkg.name)}
            {@const installedPackage = installedSource(pkg.name)}
            <li>
              <div class="entry-main">
                <strong>{pkg.name}</strong>
                <small>{pkg.description}</small>
                <span class="market-meta">
                  <span>{pkg.types.join(" · ") || "Pi Package"}</span>
                  <span>{t("{count} / 月 · {date}", { count: formatDownloads(pkg.downloads), date: formatDate(pkg.publishedAt) })}</span>
                  {#if installedPackage}<span class="market-flag">{t("已安装 · DeepPi 托管")}</span>{/if}
                </span>
              </div>
              {#if installedPackage}
                <button class="danger-action" type="button" disabled={busyPackage !== null}
                  aria-label={t("卸载 {name}", { name: pkg.name })} title={t("卸载")}
                  onclick={() => void remove(installedPackage)}><Trash2 size={14} /></button>
              {:else}
                <button class="primary-action compact" type="button" disabled={busyPackage !== null} onclick={() => void install(pkg)}>
                  {#if busyPackage === pkg.name}<span class="spin"><RefreshCw size={12} /></span>{:else}<PackageOpen size={12} />{/if}{t("安装")}
                </button>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
  {#if statusMessage}<p class="status" role="status">{statusMessage}</p>{/if}
  {#if operationState}
    <button class="secondary-action" type="button" disabled={operationState.cancelling} onclick={() => void cancelOperation()}>
      <X size={14} />{operationState.cancelling ? t("正在取消并恢复") : t("取消操作")}
    </button>
  {/if}
</section>

<style>
  .market-page { display: grid; align-content: start; gap: 16px; min-width: 0; color: var(--text); font-family: var(--text-font); }
  .market-header, .page-controls, .group-header, .header-actions, .market-toolbar, .market-search, .entry-list li, .market-meta { display: flex; align-items: center; }
  .market-header { gap: 10px; }
  h1 { margin: 0; font-size: 16px; }
  .page-controls, .group-header { justify-content: space-between; gap: 8px; }
  .page-controls { flex-wrap: wrap; }
  .scope-switch { display: inline-flex; gap: 2px; padding: 2px; border: 1px solid var(--border-strong); border-radius: 5px; background: var(--page-bg); }
  .scope-switch button { min-width: 68px; padding: 6px 12px; border: 0; border-radius: 3px; color: var(--text-muted); background: transparent; font-size: 13px; cursor: pointer; }
  .scope-switch button.active { color: var(--accent-ink); background: var(--accent); font-weight: 700; }
  button:disabled { opacity: .45; cursor: default; }
  h3 { margin: 0; font-size: 14px; font-weight: 650; color: var(--text-strong); }
  .header-actions { gap: 4px; }
  .icon-action, .danger-action { display: grid; place-items: center; flex-shrink: 0; width: 32px; height: 32px; border: 1px solid transparent; border-radius: 4px; background: transparent; color: var(--text-muted); cursor: pointer; }
  .icon-action:hover:not(:disabled) { border-color: var(--border-strong); color: var(--text); background: var(--surface-hover); }
  .danger-action { color: var(--status-failed); }
  .danger-action:hover:not(:disabled) { border-color: var(--status-failed); background: var(--surface-hover); }
  .muted { color: var(--text-muted); font-size: 12px; }
  .entry-list { list-style: none; margin: 8px 0 0; padding: 0; display: grid; gap: 6px; }
  .entry-list li { gap: 10px; min-width: 0; padding: 10px 12px; border: 1px solid var(--border); border-radius: 4px; background: var(--surface); }
  .entry-main { flex: 1; min-width: 0; display: grid; gap: 2px; }
  .entry-main strong, .entry-main small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .entry-main strong { font-size: 13px; color: var(--text-strong); }
  .entry-main small { font-size: 12px; color: var(--text-muted); }
  .market-meta { flex-wrap: wrap; gap: 8px; font-size: 12px; color: var(--text-muted); }
  .market-flag { padding: 3px 7px; border: 1px solid var(--border-strong); border-radius: 4px; color: var(--text); }
  .market-toolbar { gap: 8px; margin-top: 10px; }
  .market-search { flex: 1; min-width: 0; gap: 8px; padding: 2px 4px 2px 10px; border: 1px solid var(--border-strong); border-radius: 5px; background: var(--surface); color: var(--text-muted); }
  .market-search input { flex: 1; min-width: 0; height: 34px; border: 0; outline: 0; background: transparent; color: var(--text); font: inherit; font-size: 13px; }
  .market-sort { flex-shrink: 0; height: 32px; max-width: 180px; padding: 0 6px; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface); color: var(--text); font: inherit; font-size: 12px; cursor: pointer; }
  .primary-action, .secondary-action { display: inline-flex; align-items: center; justify-content: center; gap: 5px; min-height: 32px; padding: 0 12px; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface-hover); color: var(--text); cursor: pointer; }
  .primary-action.compact, .secondary-action { font-size: 12px; flex-shrink: 0; }
  @container (max-width: 720px) {
    .market-toolbar, .market-list li { flex-wrap: wrap; }
    .market-toolbar .market-search, .market-list .entry-main { flex-basis: 100%; }
  }
  .primary-action:hover:not(:disabled), .secondary-action:hover:not(:disabled) { border-color: var(--accent); color: var(--text-strong); }
  .secondary-action { justify-self: start; background: transparent; }
  .status { margin: 0; font-size: 13px; color: var(--accent); }
  .spin { display: inline-grid; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 480px) {
    .scope-switch button { min-width: 56px; padding-inline: 8px; }
    .market-search .primary-action { padding-inline: 8px; }
  }
</style>
