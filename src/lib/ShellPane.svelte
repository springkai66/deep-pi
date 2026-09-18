<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { FitAddon } from "@xterm/addon-fit";
  import { Terminal } from "@xterm/xterm";
  import "@xterm/xterm/css/xterm.css";
  import { onMount } from "svelte";
  import { X } from "@lucide/svelte";
  import {
    cssTerminalFontFamily,
    isLightColorMode,
    FONT_SIZE_RANGE,
    type CodeFont,
    type ColorMode,
    type TerminalShell,
  } from "$lib/settings";
  import { t, tm } from "$lib/i18n.svelte";

  interface ShellOutput {
    sessionId: string;
    runId: string;
    data: string;
  }

  interface ShellExit {
    sessionId: string;
    runId: string;
    exitCode: number | null;
    error: string | null;
  }

  interface ShellSessionInfo {
    sessionId: string;
    runId: string;
    shell: TerminalShell;
    cwd: string;
  }

  interface Props {
    projectId: string;
    shell: TerminalShell;
    codeFont: CodeFont;
    sessionFontName: string;
    sessionFontSize: number;
    colorMode: ColorMode;
    onClose: () => void;
  }

  let { projectId, shell, codeFont, sessionFontName, sessionFontSize, colorMode, onClose }: Props = $props();
  let container: HTMLDivElement;
  let terminal: Terminal | undefined;
  let sessionId = $state<string | null>(null);
  let runId = $state<string | null>(null);
  let shellName = $state<TerminalShell | null>(null);
  let cwd = $state("");
  let startupError = $state("");
  let disposed = false;

  function shellLabel(value: TerminalShell): string {
    return value === "powershell" ? "PowerShell"
      : value === "pwsh" ? "PowerShell 7"
      : value === "bash" ? "Bash"
      : t("命令提示符");
  }

  function terminalTheme(light: boolean) {
    return light
      ? {
          background: "#ffffff", foreground: "#253128", cursor: "#2f8a5d", cursorAccent: "#ffffff",
          selectionBackground: "#b8d8c2", black: "#253128", red: "#b33a32", green: "#24734a",
          yellow: "#8a6612", blue: "#356fa3", magenta: "#7b4b96", cyan: "#247c7b",
          white: "#f3f6f4", brightBlack: "#66756b", brightRed: "#c64d43", brightGreen: "#2f8a5d",
          brightYellow: "#9d7414", brightBlue: "#4788bf", brightMagenta: "#915ab0", brightCyan: "#309897",
          brightWhite: "#142119",
        }
      : {
          background: "#111412", foreground: "#d8ded9", cursor: "#7dd3a7", cursorAccent: "#111412",
          selectionBackground: "#375c4a", black: "#111412", red: "#ef7b72", green: "#7dd3a7",
          yellow: "#e4c875", blue: "#78a8d8", magenta: "#c49ad9", cyan: "#74c7c4",
          white: "#d8ded9", brightBlack: "#626a64", brightRed: "#ff9c92", brightGreen: "#9be7bf",
          brightYellow: "#f5dc91", brightBlue: "#9bc5ed", brightMagenta: "#deb4f0", brightCyan: "#93dfdc",
          brightWhite: "#f4f7f5",
        };
  }

  $effect(() => {
    if (!terminal) return;
    terminal.options.fontFamily = cssTerminalFontFamily(sessionFontName, codeFont);
    terminal.options.fontSize = Math.round(Math.min(FONT_SIZE_RANGE.max, Math.max(FONT_SIZE_RANGE.min, sessionFontSize)));
    terminal.options.theme = terminalTheme(isLightColorMode(colorMode));
    requestAnimationFrame(() => {
      if (!terminal) return;
      terminal.clearTextureAtlas();
      terminal.refresh(0, Math.max(0, terminal.rows - 1));
    });
  });

  function decodeBase64(value: string): Uint8Array {
    const binary = atob(value);
    return Uint8Array.from(binary, (character) => character.charCodeAt(0));
  }

  function showError(message: unknown) {
    startupError = tm(String(message));
    terminal?.writeln(`\r\n\x1b[31m${startupError}\x1b[0m`);
  }

  onMount(() => {
    const terminalInstance = new Terminal({
      allowProposedApi: false,
      convertEol: false,
      cursorBlink: true,
      cursorStyle: "bar",
      fontFamily: cssTerminalFontFamily(sessionFontName, codeFont),
      fontSize: Math.round(Math.min(FONT_SIZE_RANGE.max, Math.max(FONT_SIZE_RANGE.min, sessionFontSize))),
      lineHeight: 1.2,
      letterSpacing: 0,
      fontWeight: 400,
      fontWeightBold: 600,
      scrollback: 5_000,
      theme: terminalTheme(isLightColorMode(colorMode)),
    });
    terminal = terminalInstance;
    const fitAddon = new FitAddon();
    terminalInstance.loadAddon(fitAddon);
    terminalInstance.open(container);

    let lastRows = 0;
    let lastCols = 0;
    let resizePending = false;
    let outputUnlisten: (() => void) | undefined;
    let exitUnlisten: (() => void) | undefined;
    const pendingOutput: ShellOutput[] = [];

    const writeOutput = (event: ShellOutput) => {
      if (event.sessionId === sessionId && event.runId === runId) {
        terminalInstance.write(decodeBase64(event.data));
      } else if (!sessionId && pendingOutput.length < 32) {
        pendingOutput.push(event);
      }
    };

    const handleExit = (event: ShellExit) => {
      if (event.sessionId !== sessionId || event.runId !== runId) return;
      terminalInstance.options.cursorBlink = false;
      terminalInstance.writeln("");
      terminalInstance.writeln(
        event.error
          ? `\x1b[31m${tm(event.error)}\x1b[0m`
          : `\x1b[90m${t("Shell 已退出（代码 {code}）。", { code: event.exitCode ?? "unknown" })}\x1b[0m`,
      );
      sessionId = null;
      runId = null;
    };

    const resize = () => {
      if (disposed) return;
      fitAddon.fit();
      if (terminalInstance.rows === lastRows && terminalInstance.cols === lastCols) return;
      lastRows = terminalInstance.rows;
      lastCols = terminalInstance.cols;
      if (!sessionId || resizePending) return;
      resizePending = true;
      void invoke("resize_shell", { sessionId, rows: lastRows, cols: lastCols })
        .catch((error) => { if (!disposed) showError(error); })
        .finally(() => { resizePending = false; });
    };

    const resizeObserver = new ResizeObserver(resize);
    resizeObserver.observe(container);
    resize();

    const dataDisposable = terminalInstance.onData((data) => {
      if (!sessionId || !runId || disposed) return;
      void invoke("write_shell", { sessionId, runId, data }).catch((error) => {
        if (!disposed) terminalInstance.writeln(`\r\n\x1b[31m${tm(String(error))}\x1b[0m`);
      });
    });
    terminalInstance.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown" || !event.ctrlKey || !event.shiftKey) return true;
      if (event.key.toLowerCase() === "c") {
        const selection = terminalInstance.getSelection();
        if (selection && navigator.clipboard) void navigator.clipboard.writeText(selection).catch(() => {});
        return false;
      }
      if (event.key.toLowerCase() === "v") {
        if (navigator.clipboard) void navigator.clipboard.readText().then((text) => terminalInstance.paste(text)).catch(() => {});
        return false;
      }
      return true;
    });

    const setup = async () => {
      try {
        [outputUnlisten, exitUnlisten] = await Promise.all([
          listen<ShellOutput>("shell-output", ({ payload }) => writeOutput(payload)),
          listen<ShellExit>("shell-exit", ({ payload }) => handleExit(payload)),
        ]);
        const started = await invoke<ShellSessionInfo>("start_shell", {
          request: { projectId, rows: terminalInstance.rows || 24, cols: terminalInstance.cols || 80 },
        });
        if (disposed) {
          await invoke("stop_shell", { sessionId: started.sessionId, runId: started.runId }).catch(() => {});
          return;
        }
        sessionId = started.sessionId;
        runId = started.runId;
        shellName = started.shell;
        cwd = started.cwd;
        for (const event of pendingOutput.splice(0)) writeOutput(event);
        resize();
        terminalInstance.focus();
      } catch (error) {
        if (!disposed) showError(error);
      }
    };
    void setup();

    return () => {
      disposed = true;
      dataDisposable.dispose();
      resizeObserver.disconnect();
      outputUnlisten?.();
      exitUnlisten?.();
      const currentSession = sessionId;
      const currentRun = runId;
      if (currentSession && currentRun) {
        void invoke("stop_shell", { sessionId: currentSession, runId: currentRun }).catch(() => {});
      }
      terminalInstance.dispose();
      terminal = undefined;
    };
  });
</script>

<section class="shell-pane" aria-label={t("命令终端")}>
  <header>
    <div class="shell-heading">
      <strong>{t("命令终端")}</strong>
      <span>{shellLabel(shellName ?? shell)}{#if cwd} · {cwd}{/if}</span>
    </div>
    <button type="button" aria-label={t("关闭命令终端")} title={t("关闭命令终端")} onclick={onClose}><X size={16} /></button>
  </header>
  {#if startupError}<p class="shell-status" role="alert">{startupError}</p>{/if}
  <div class="terminal" bind:this={container}></div>
</section>

<style>
  .shell-pane { display: grid; grid-template-rows: 36px minmax(0, 1fr); min-width: 0; min-height: 0; height: 100%; overflow: hidden; border: 1px solid #303832; border-radius: 6px; background: #111412; font-family: var(--code-font); }
  header { display: flex; align-items: center; gap: 8px; padding: 0 10px; overflow: hidden; border-bottom: 1px solid #303832; color: #aeb7b0; background: #191d1a; font-size: 12px; }
  .shell-heading { min-width: 0; display: flex; align-items: baseline; gap: 8px; flex: 1; overflow: hidden; }
  .shell-heading strong { color: #d8ded9; flex-shrink: 0; }
  .shell-heading span { min-width: 0; overflow: hidden; color: #89958c; text-overflow: ellipsis; white-space: nowrap; }
  header button { display: grid; place-items: center; width: 26px; height: 26px; padding: 0; flex-shrink: 0; border: 0; border-radius: 4px; color: inherit; background: transparent; cursor: pointer; }
  header button:hover { background: var(--surface-hover); }
  .terminal { min-width: 0; min-height: 0; padding: 6px 4px 2px; }
  .terminal :global(.xterm) { height: 100%; }
  .shell-status { position: absolute; top: 44px; right: 12px; z-index: 1; max-width: min(520px, 70%); margin: 0; padding: 6px 8px; border: 1px solid #74423e; border-radius: 4px; color: #ffd2ce; background: #3b201e; font-size: 11px; overflow-wrap: anywhere; }
</style>
