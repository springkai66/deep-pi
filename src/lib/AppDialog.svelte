<script lang="ts">
  import { X } from "@lucide/svelte";
  import { tick } from "svelte";
  import type { DialogRequest, DialogValue } from "$lib/dialog";

  interface Props {
    request: DialogRequest | null;
    onResolve: (value: DialogValue) => void;
  }

  let { request, onResolve }: Props = $props();
  let inputValue = $state("");
  let inputElement = $state<HTMLInputElement>();
  let choiceButton = $state<HTMLButtonElement>();
  let confirmButton = $state<HTMLButtonElement>();
  let initializedRequestId: number | null = null;

  $effect(() => {
    if (!request || initializedRequestId === request.id) return;
    initializedRequestId = request.id;
    inputValue = request.initialValue ?? "";
    void tick().then(() => {
      if (request?.kind === "input") inputElement?.focus();
      else if (request?.kind === "choice") choiceButton?.focus();
      else confirmButton?.focus();
    });
  });

  function resolve(value: DialogValue) {
    if (request) onResolve(value);
  }

  function cancel() {
    resolve(request?.kind === "confirm" ? false : null);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      cancel();
    }
  }
</script>

{#if request}
  <div
    class="dialog-backdrop"
    role="presentation"
    onclick={(event) => event.target === event.currentTarget && cancel()}
  >
    <dialog
      class="app-dialog"
      open
      aria-modal="true"
      aria-labelledby={`dialog-title-${request.id}`}
      aria-describedby={`dialog-message-${request.id}`}
      onkeydown={handleKeydown}
    >
      <header>
        <h2 id={`dialog-title-${request.id}`}>{request.title}</h2>
        <button class="icon-button" type="button" aria-label="关闭" title="关闭" onclick={cancel}>
          <X size={17} />
        </button>
      </header>

      <p id={`dialog-message-${request.id}`}>{request.message}</p>

      {#if request.kind === "choice"}
        <div class="dialog-choices" role="group" aria-label="关闭行为">
          {#each request.choices ?? [] as choice}
            <button bind:this={choiceButton} class="choice-button" type="button" onclick={() => resolve(choice.value)}>
              {choice.label}
            </button>
          {/each}
        </div>
      {:else}
        {#if request.kind === "input"}
          <input
            bind:this={inputElement}
            class="dialog-input"
            value={inputValue}
            placeholder={request.placeholder ?? ""}
            maxlength="200"
            aria-label={request.title}
            oninput={(event) => (inputValue = event.currentTarget.value)}
          />
        {/if}

        <form
          onsubmit={(event) => {
            event.preventDefault();
            resolve(request.kind === "input" ? inputValue : true);
          }}
        >
          {#if request.kind !== "alert"}
            <button class="secondary-button" type="button" onclick={cancel}>取消</button>
          {/if}
          <button bind:this={confirmButton} class="primary-button" type="submit">
            {request.confirmLabel ?? (request.kind === "alert" ? "知道了" : "确认")}
          </button>
        </form>
      {/if}
    </dialog>
  </div>
{/if}

<style>
  .dialog-backdrop {
    position: fixed;
    z-index: 20;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgb(3 6 4 / 68%);
  }

  .app-dialog {
    width: min(440px, 100%);
    padding: 18px;
    border: 1px solid #48534b;
    border-radius: 7px;
    color: #d8ded9;
    background: #1b211d;
    font-family: var(--text-font);
    box-shadow: 0 18px 48px rgb(0 0 0 / 38%);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }

  h2 {
    margin: 0;
    color: #f4f7f5;
    font-size: 16px;
    font-weight: 650;
  }

  p {
    margin: 14px 0 16px;
    color: #aeb7b0;
    line-height: 1.55;
    white-space: pre-wrap;
  }

  .icon-button,
  .secondary-button,
  .primary-button {
    border-radius: 4px;
    cursor: pointer;
  }

  .icon-button {
    display: inline-grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 1px solid transparent;
    color: #9aa49c;
    background: transparent;
  }

  .icon-button:hover {
    border-color: #465048;
    color: #f4f7f5;
    background: #252b27;
  }

  .dialog-input {
    width: 100%;
    height: 34px;
    padding: 0 10px;
    border: 1px solid #465048;
    border-radius: 4px;
    outline: none;
    color: #f4f7f5;
    background: #111412;
  }

  .dialog-input:focus {
    border-color: #8fd6ad;
  }

  .dialog-choices {
    display: grid;
    gap: 8px;
    margin-top: 18px;
  }

  .choice-button {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    min-height: 34px;
    padding: 0 12px;
    border: 1px solid #465048;
    border-radius: 4px;
    color: #d8ded9;
    background: #252b27;
    cursor: pointer;
    font-weight: 600;
  }

  .choice-button:hover,
  .choice-button:focus-visible {
    border-color: #8fd6ad;
    color: #111412;
    background: #8fd6ad;
  }

  form {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 18px;
  }

  .secondary-button,
  .primary-button {
    min-width: 72px;
    height: 32px;
    padding: 0 12px;
    border: 1px solid #465048;
    font-weight: 600;
  }

  .secondary-button {
    color: #c0c8c1;
    background: #252b27;
  }

  .primary-button {
    border-color: #8fd6ad;
    color: #111412;
    background: #8fd6ad;
  }
</style>
