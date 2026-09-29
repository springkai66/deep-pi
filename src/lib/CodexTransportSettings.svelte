<script lang="ts">
  import { CODEX_TRANSPORT_OPTIONS, type CodexTransport } from "./settings";
  import { t } from "$lib/i18n.svelte";

  let { value, onChange }: { value: CodexTransport; onChange: (value: CodexTransport) => void } = $props();
</script>

<section class="codex-transport" aria-labelledby="codex-transport-heading">
  <h3 id="codex-transport-heading">{t("Codex 传输方式")}</h3>
  <label><strong>{t("连接方式")}</strong>
    <select value={value} aria-label={t("Codex 传输方式")} aria-describedby="codex-transport-description codex-transport-scope"
      onchange={(event) => onChange(event.currentTarget.value as CodexTransport)}>
      {#each CODEX_TRANSPORT_OPTIONS as option}
        <option value={option.value}>{t(option.label)}</option>
      {/each}
    </select>
  </label>
  <p id="codex-transport-description">{t(CODEX_TRANSPORT_OPTIONS.find((option) => option.value === value)?.description ?? "")}</p>
  <p id="codex-transport-scope">{t("仅作用于 DeepPi 的 Codex 聊天会话，优先于 Pi 自身的传输设置。自动保存；新建或重新启动会话后生效，正在运行的会话保持原方式。")}</p>
</section>

<style>
  .codex-transport { flex-shrink: 0; display: grid; gap: 8px; padding: 12px 14px; border: 1px solid var(--border); border-radius: 6px; background: var(--surface-alt); }
  h3 { margin: 0; font-size: 13px; color: var(--text-strong); }
  label { display: flex; align-items: center; justify-content: space-between; gap: 16px; flex-wrap: wrap; font-size: 12px; }
  select { max-width: 100%; min-height: 32px; padding: 5px 9px; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface); color: var(--text); font: inherit; }
  p { margin: 0; color: var(--text-muted); font-size: 11px; line-height: 1.6; overflow-wrap: anywhere; }
</style>
