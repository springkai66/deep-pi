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
  let query = $state("");
  let packages = $state<PiPackage[]>([]);
  let installed = $state<InstalledPackage[]>([]);
  let scope = $state<"global" | "project">("global");
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
    try {
      const result = await invoke<InstalledPackage[]>("list_pi_packages", {
        scope,
        projectPath,
        environment: "managed",
      });
      if (generation === installedGeneration) installed = result;
    } catch (error) {
      if (generation === installedGeneration) onError(error);
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
  <header class="market-header">
    {#if !embedded}<div class="market-title">
      <button class="icon-button" type="button" aria-label={t("返回工作区")} title={t("返回")} disabled={busyPackage !== null} onclick={onClose}>
        <X size={17} />
      </button>
      <PackageOpen size={18} />
      <h1>{t("Pi 扩展市场")}</h1>
    </div>{/if}
    <div class="scope-switch" aria-label={t("安装范围")}>
      <button class:active={scope === "global"} type="button" disabled={busyPackage !== null} onclick={() => { scope = "global"; void refreshInstalled(); }}>{t("全局")}</button>
      <button class:active={scope === "project"} type="button" disabled={!canUseProjectScope || busyPackage !== null} onclick={() => { scope = "project"; void refreshInstalled(); }}>{t("项目")}</button>
    </div>
  </header>

  <form class="market-search" onsubmit={(event) => { event.preventDefault(); void search(); }}>
    <Search size={16} />
    <input bind:value={query} aria-label={t("搜索 Pi Package")} placeholder={t("搜索 Pi Package")} />
    <button type="submit" aria-label={t("搜索")} title={t("搜索")}><Search size={15} /></button>
  </form>

  {#if statusMessage}<p class="market-status" role="status">{statusMessage}</p>{/if}
  {#if operationState}
    <button class="quiet-button" type="button" disabled={operationState.cancelling} onclick={() => void cancelOperation()}><X size={14} />{operationState.cancelling ? t("正在取消并恢复") : t("取消操作")}</button>
  {/if}

  <div class="market-body">
    <section class="package-list" aria-busy={isLoading}>
      <header class="section-heading"><span>{t("官方目录")}</span><span>{packages.length}</span></header>
      {#if isLoading}
        <div class="market-empty">{t("正在加载目录…")}</div>
      {:else if packages.length === 0}
        <div class="market-empty">{t("没有匹配的 Package")}</div>
      {:else}
        <div class="package-grid">
          {#each packages as pkg (pkg.name)}
            {@const installedPackage = installedSource(pkg.name)}
            <article class="package-card">
              <div class="package-card-header">
                <div>
                  <h2>{pkg.name}</h2>
                  <span class="package-types">{pkg.types.join(" · ") || "Pi Package"}</span>
                </div>
                {#if installedPackage}
                  <span class="installed-badge">{t("已安装 · DeepPi 托管")}</span>
                {/if}
              </div>
              <p>{pkg.description}</p>
              <footer>
                <span>{t("{count} / 月 · {date}", { count: formatDownloads(pkg.downloads), date: formatDate(pkg.publishedAt) })}</span>
                {#if installedPackage}
                  <button class="danger-button" type="button" disabled={busyPackage !== null} onclick={() => void remove(installedPackage)}>
                    <Trash2 size={13} />{t("卸载")}
                  </button>
                {:else}
                  <button class="primary-button" type="button" disabled={busyPackage !== null} onclick={() => void install(pkg)}>
                    {#if busyPackage === pkg.name}<span class="spin"><RefreshCw size={13} /></span>{:else}<PackageOpen size={13} />{/if}
                    {t("安装")}
                  </button>
                {/if}
              </footer>
            </article>
          {/each}
        </div>
      {/if}
    </section>

    <aside class="installed-panel">
      <header class="section-heading">
        <span>{t("已安装")} <small class="environment-note">{scope === "global" ? t("DeepPi 托管") : t("当前项目")}</small></span>
        <span class="section-heading-actions">
          <button type="button" aria-label={t("全部更新 Pi Package")} title={t("更新 DeepPi 托管扩展")} disabled={busyPackage !== null || installed.length === 0} onclick={() => void updateAll()}><ArrowUpCircle size={14} /></button>
          <button type="button" aria-label={t("刷新已安装 Package")} title={t("刷新")} onclick={() => void refreshInstalled()}><RefreshCw size={14} /></button>
        </span>
      </header>
      {#if installed.length === 0}
        <p class="market-empty">{t("暂无已安装 Package")}</p>
      {:else}
        {#each installed as pkg (`${pkg.source}`)}
          <div class="installed-row">
            <span class="installed-copy">
              <strong>{pkg.source}</strong>
              <small>{t("DeepPi 托管")}</small>
            </span>
            <button type="button" aria-label={t("更新 {name}", { name: pkg.source })} title={t("更新")} disabled={busyPackage !== null} onclick={() => void update(pkg)}><RefreshCw size={13} /></button>
          </div>
        {/each}
      {/if}
    </aside>
  </div>
</section>

<style>
  .market-page.embedded { padding: 0; display: flex; flex-direction: column; }
  .embedded .market-header { justify-content: flex-end; }
  .embedded .market-header, .embedded .market-search { flex-shrink: 0; }
  .embedded .market-body { flex: 1; }
  .market-page {
    display: grid;
    grid-template-rows: 54px 42px auto minmax(0, 1fr);
    gap: 12px;
    height: 100%;
    padding: 14px 18px 18px;
    overflow: hidden;
    color: #d8ded9;
    font-family: var(--text-font);
  }

  .market-header,
  .market-title,
  .scope-switch,
  .market-search,
  .package-card-header,
  .package-card footer,
  .section-heading,
  .installed-row {
    display: flex;
    align-items: center;
  }

  .market-header,
  .package-card footer,
  .section-heading {
    justify-content: space-between;
  }

  .market-title {
    gap: 8px;
  }

  h1,
  h2,
  p {
    margin: 0;
  }

  h1 {
    font-size: 16px;
    font-weight: 650;
  }

  h2 {
    overflow: hidden;
    color: #f4f7f5;
    font-size: 13px;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .icon-button,
  .market-search button,
  .scope-switch button,
  .installed-panel button,
  .primary-button,
  .danger-button {
    border-radius: 4px;
    cursor: pointer;
  }

  .icon-button {
    display: inline-grid;
    place-items: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 1px solid transparent;
    color: #aeb7b0;
    background: transparent;
  }

  .icon-button:hover,
  .installed-panel button:hover {
    border-color: #465048;
    color: #f4f7f5;
    background: #252b27;
  }

  .scope-switch {
    gap: 2px;
    padding: 2px;
    border: 1px solid #39423b;
    border-radius: 5px;
  }

  .scope-switch button {
    height: 26px;
    padding: 0 10px;
    border: 0;
    color: #89928b;
    background: transparent;
  }

  .scope-switch button.active {
    color: #111412;
    background: #8fd6ad;
    font-weight: 700;
  }

  .scope-switch button:disabled {
    cursor: default;
    opacity: 0.4;
  }

  .market-search {
    gap: 8px;
    padding: 0 10px;
    border: 1px solid #39423b;
    border-radius: 5px;
    color: #778078;
    background: #161a17;
  }

  .market-search input {
    flex: 1;
    min-width: 0;
    height: 36px;
    border: 0;
    outline: 0;
    color: #d8ded9;
    background: transparent;
  }

  .market-search button {
    display: inline-grid;
    place-items: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    color: #aeb7b0;
    background: transparent;
  }

  .market-search button:hover {
    color: #f4f7f5;
    background: #252b27;
  }

  .market-status {
    color: #8fd6ad;
    font-size: 12px;
  }

  .market-body {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 230px;
    gap: 14px;
    min-height: 0;
    overflow: hidden;
  }

  .package-list,
  .installed-panel {
    min-width: 0;
    min-height: 0;
    overflow: auto;
  }

  .section-heading {
    min-height: 28px;
    padding: 0 2px;
    color: #89928b;
    font-size: 11px;
    font-weight: 700;
  }

.section-heading-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .section-heading button {
    display: inline-grid;
    place-items: center;
    width: 25px;
    height: 25px;
    padding: 0;
    border: 1px solid transparent;
    color: #89928b;
    background: transparent;
  }

  .package-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
    gap: 8px;
  }

  .package-card {
    display: grid;
    grid-template-rows: auto minmax(50px, 1fr) auto;
    min-width: 0;
    min-height: 158px;
    padding: 12px;
    border: 1px solid #303832;
    border-radius: 6px;
    background: #191d1a;
  }

  .package-card-header {
    gap: 8px;
    min-width: 0;
  }

  .package-types,
  .package-card footer,
  .installed-row {
    color: #778078;
    font-size: 10px;
  }

  .environment-note {
    margin-left: 4px;
    color: #607067;
    font-size: 9px;
    font-weight: 400;
  }

  .package-card p {
    display: -webkit-box;
    margin: 10px 0;
    overflow: hidden;
    color: #aeb7b0;
    line-height: 1.45;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
  }

  .installed-badge {
    flex: 0 0 auto;
    padding: 3px 6px;
    border: 1px solid #39654d;
    border-radius: 3px;
    color: #8fd6ad;
    font-size: 10px;
  }

  .primary-button,
  .danger-button {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 27px;
    padding: 0 8px;
    border: 1px solid transparent;
    font-size: 11px;
    font-weight: 650;
  }

  .primary-button {
    color: #111412;
    background: #8fd6ad;
  }

  .danger-button {
    border-color: #70423e;
    color: #f2aaa3;
    background: transparent;
  }

  .primary-button:disabled,
  .danger-button:disabled {
    cursor: default;
    opacity: 0.45;
  }

  .installed-panel {
    padding-left: 12px;
    border-left: 1px solid #303832;
  }

  .installed-row {
    justify-content: space-between;
    gap: 8px;
    min-height: 36px;
    border-bottom: 1px solid #262c27;
  }

  .installed-row span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .installed-copy {
    display: grid;
    min-width: 0;
    gap: 3px;
  }

  .installed-copy strong {
    overflow: hidden;
    color: #aeb7b0;
    font-size: 11px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .installed-copy small {
    color: #667169;
    font-size: 9px;
  }

  .installed-panel button {
    display: inline-grid;
    place-items: center;
    flex: 0 0 auto;
    width: 25px;
    height: 25px;
    padding: 0;
    border: 1px solid transparent;
    color: #89928b;
    background: transparent;
  }

  .installed-panel button:disabled {
    cursor: default;
    opacity: 0.45;
  }

  .market-empty {
    padding: 24px 2px;
    color: #778078;
    font-size: 12px;
  }

  .spin {
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  @media (max-width: 900px) {
    .market-body {
      grid-template-columns: minmax(0, 1fr);
      overflow: auto;
    }

    .installed-panel {
      padding: 8px 0 0;
      border-top: 1px solid #303832;
      border-left: 0;
    }
  }
  @container (max-width: 680px) {
    .market-page.embedded { height: auto; overflow: visible; }
    .embedded .market-body { grid-template-columns: minmax(0, 1fr); overflow: visible; }
    .embedded .package-list, .embedded .installed-panel { overflow: visible; }
    .embedded .installed-panel { padding: 12px 0 0; border-top: 1px solid var(--border); border-left: 0; }
    .embedded .package-grid { grid-template-columns: repeat(auto-fill, minmax(min(250px, 100%), 1fr)); }
  }
</style>
