<script lang="ts">
  import { createTerminalInput } from "./terminal-input";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { FitAddon } from "@xterm/addon-fit";
  import { Terminal, type ITerminalAddon } from "@xterm/xterm";
  import "@xterm/xterm/css/xterm.css";
  import { onMount } from "svelte";
  import { Bot } from "@lucide/svelte";
  import {
    cssTerminalFontFamily,
    isLightColorMode,
    FONT_SIZE_RANGE,
    type CodeFont,
    type ColorMode,
  } from "$lib/settings";
  import type { TaskStatus } from "$lib/task";
  import { createTerminalSession } from "$lib/terminal-session";
  import { t, tm } from "$lib/i18n.svelte";

  interface Props {
    taskId: string;
    runId?: string | null;
    title: string;
    status: TaskStatus;
    visible: boolean;
    active: boolean;
    transitioning: boolean;
    codeFont: CodeFont;
    sessionFontName: string;
    sessionFontSize: number;
    colorMode: ColorMode;
    onExit: (exitCode: number | null, error: string | null, runId: string) => void;
    onUseConversation: () => void;
  }

  let { taskId, runId = null, title, status, visible, active, transitioning, codeFont, sessionFontName, sessionFontSize, colorMode, onExit, onUseConversation }: Props = $props();
  let container: HTMLDivElement;
  let terminal: Terminal | undefined;
  let session = $state<ReturnType<typeof createTerminalSession> | null>(null);
  let resizeTerminal: () => void = () => {};

  $effect(() => {
    session?.setRun(runId);
  });

  function terminalTheme(light: boolean) {
    return light
      ? {
          background: "#ffffff",
          foreground: "#253128",
          cursor: "#2f8a5d",
          cursorAccent: "#ffffff",
          selectionBackground: "#b8d8c2",
          black: "#253128",
          red: "#b33a32",
          green: "#24734a",
          yellow: "#8a6612",
          blue: "#356fa3",
          magenta: "#7b4b96",
          cyan: "#247c7b",
          white: "#f3f6f4",
          brightBlack: "#66756b",
          brightRed: "#c64d43",
          brightGreen: "#2f8a5d",
          brightYellow: "#9d7414",
          brightBlue: "#4788bf",
          brightMagenta: "#915ab0",
          brightCyan: "#309897",
          brightWhite: "#142119",
        }
      : {
          background: "#111412",
          foreground: "#d8ded9",
          cursor: "#7dd3a7",
          cursorAccent: "#111412",
          selectionBackground: "#375c4a",
          black: "#111412",
          red: "#ef7b72",
          green: "#7dd3a7",
          yellow: "#e4c875",
          blue: "#78a8d8",
          magenta: "#c49ad9",
          cyan: "#74c7c4",
          white: "#d8ded9",
          brightBlack: "#626a64",
          brightRed: "#ff9c92",
          brightGreen: "#9be7bf",
          brightYellow: "#f5dc91",
          brightBlue: "#9bc5ed",
          brightMagenta: "#deb4f0",
          brightCyan: "#93dfdc",
          brightWhite: "#f4f7f5",
        };
  }

  $effect(() => {
    if (!visible || !terminal) return;
    terminal.options.cursorBlink = ["running", "waiting"].includes(status);
    requestAnimationFrame(() => {
      resizeTerminal();
      if (active) terminal?.focus();
    });
  });

  $effect(() => {
    if (!terminal) return;
    terminal.options.fontFamily = cssTerminalFontFamily(sessionFontName, codeFont);
    terminal.options.fontSize = Math.round(Math.min(FONT_SIZE_RANGE.max, Math.max(FONT_SIZE_RANGE.min, sessionFontSize)));
    terminal.options.theme = terminalTheme(isLightColorMode(colorMode));
    const refreshLayout = () => {
      if (!terminal) return;
      terminal.clearTextureAtlas();
      terminal.refresh(0, Math.max(0, terminal.rows - 1));
      resizeTerminal();
    };
    requestAnimationFrame(refreshLayout);
    void document.fonts.ready.then(refreshLayout).catch(() => {});
  });

  function decodeBase64(value: string): Uint8Array {
    const binary = atob(value);
    return Uint8Array.from(binary, (character) => character.charCodeAt(0));
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

    let imageAddon: ITerminalAddon | null = null;
    let lastRows = 0;
    let lastCols = 0;
    let disposed = false;
    void import("@xterm/addon-image").then(({ ImageAddon }) => {
      if (disposed) return;
      imageAddon = new ImageAddon({
        enableSizeReports: true,
        pixelLimit: 8_388_608,
        storageLimit: 32,
        sixelSizeLimit: 4_000_000,
        iipSizeLimit: 4_000_000,
      });
      terminalInstance.loadAddon(imageAddon);
    }).catch(() => {});

    const connection = createTerminalSession(taskId, {
      listen: <T,>(name: string, handler: (payload: T) => void) =>
        listen<T>(name, ({ payload }) => handler(payload)),
      acknowledge: (taskId, runId) => invoke("acknowledge_pi_output", { taskId, runId }),
      output: (data) => terminalInstance.write(decodeBase64(data)),
      exit: (exitCode, error, exitedRunId) => {
        terminalInstance.options.cursorBlink = false;
        terminalInstance.writeln("");
        terminalInstance.writeln(
          error
            ? `\x1b[31m${tm(error)}\x1b[0m`
            : `\x1b[90mPi exited with code ${exitCode ?? "unknown"}.\x1b[0m`,
        );
        onExit(exitCode, error, exitedRunId);
      },
      error: (error) => terminalInstance.writeln(`\r\nTerminal subscription failed: ${tm(String(error))}`),
    });
    session = connection;

    terminalInstance.open(container);
    if (active) terminalInstance.focus();

    const resize = () => {
      if (disposed || !visible) return;
      fitAddon.fit();
      if (terminalInstance.rows === lastRows && terminalInstance.cols === lastCols) return;
      lastRows = terminalInstance.rows;
      lastCols = terminalInstance.cols;
      void invoke("resize_pi_task", {
        taskId,
        rows: terminalInstance.rows,
        cols: terminalInstance.cols,
      }).catch(() => {});
    };
    resizeTerminal = resize;
    void document.fonts.ready.then(() => {
      if (disposed) return;
      terminalInstance.refresh(0, Math.max(0, terminalInstance.rows - 1));
      resize();
    }).catch(() => {});

    const resizeObserver = new ResizeObserver(resize);
    resizeObserver.observe(container);
    resize();

    const inputQueue = createTerminalInput({
      currentRun: () => transitioning ? null : runId,
      write: (runId, data) => invoke("write_pi_task", { taskId, runId, data }),
      error: (error) => {
        terminalInstance.writeln(`\r\n\x1b[31mInput failed: ${tm(String(error))}\x1b[0m`);
      },
    });
    const dataDisposable = terminalInstance.onData((data) => inputQueue.send(data));
    terminalInstance.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown" || !event.ctrlKey || !event.shiftKey) return true;
      if (event.key.toLowerCase() === "c") {
        const selection = terminalInstance.getSelection();
        if (selection && navigator.clipboard) {
          void navigator.clipboard.writeText(selection).catch(() => {});
        }
        return false;
      }
      if (event.key.toLowerCase() === "v") {
        if (navigator.clipboard) {
          void navigator.clipboard
            .readText()
            .then((text) => terminalInstance.paste(text))
            .catch(() => {});
        }
        return false;
      }
      return true;
    });

    return () => {
      disposed = true;
      inputQueue.dispose();
      connection.dispose();
      session = null;
      resizeObserver.disconnect();
      dataDisposable.dispose();
      imageAddon?.dispose();
      terminalInstance.dispose();
      terminal = undefined;
      resizeTerminal = () => {};
    };
  });
</script>

<section class:hidden={!visible} class="terminal-pane" aria-label={`${title} terminal`}>
  <header><span>{title}{#if transitioning}{t(" · 正在切换模式")}{/if}</span>
    <button type="button" title={t("切换到对话模式")} aria-label={t("切换到对话模式")} disabled={transitioning} onclick={onUseConversation}><Bot size={15} /></button>
  </header>
  <div class="terminal" bind:this={container}></div>
</section>

<style>
  .terminal-pane {
    display: grid;
    grid-template-rows: 32px minmax(0, 1fr);
    min-width: 0;
    min-height: 240px;
    overflow: hidden;
    border: 1px solid #303832;
    border-radius: 6px;
    background: #111412;
    font-family: var(--code-font);
    font-variant-ligatures: none;
    font-feature-settings: "liga" 0, "calt" 0;
  }

  .terminal-pane.hidden {
    display: none;
  }

  header {
    display: flex;
    align-items: center;
    gap: 8px;
    display: flex;
    align-items: center;
    padding: 0 10px;
    overflow: hidden;
    border-bottom: 1px solid #303832;
    color: #aeb7b0;
    background: #191d1a;
    font-size: 12px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  header span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  header button { display: grid; place-items: center; width: 26px; height: 26px; padding: 0; flex-shrink: 0; border: 0; border-radius: 4px; color: inherit; background: transparent; cursor: pointer; }
  header button:hover:not(:disabled) { background: var(--surface-hover); }
  header button:disabled { opacity: .4; cursor: default; }

  .terminal {
    min-width: 0;
    min-height: 0;
    padding: 6px 4px 2px;
  }

  .terminal :global(.xterm) {
    height: 100%;
  }
</style>
