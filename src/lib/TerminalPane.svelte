<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { FitAddon } from "@xterm/addon-fit";
  import { Terminal, type ITerminalAddon } from "@xterm/xterm";
  import "@xterm/xterm/css/xterm.css";
  import { onMount } from "svelte";
  import {
    cssFontFamily,
    isLightColorMode,
    type CodeFont,
    type ColorMode,
  } from "$lib/settings";
  import type { TaskStatus } from "$lib/task";

  interface Props {
    taskId: string;
    title: string;
    status: TaskStatus;
    visible: boolean;
    active: boolean;
    codeFont: CodeFont;
    colorMode: ColorMode;
    onExit: (exitCode: number | null, error: string | null) => void;
  }

  interface PtyOutput {
    taskId: string;
    data: string;
  }

  interface PtyExit {
    taskId: string;
    exitCode: number | null;
    error: string | null;
  }

  let { taskId, title, status, visible, active, codeFont, colorMode, onExit }: Props = $props();
  let container: HTMLDivElement;
  let terminal: Terminal | undefined;
  let resizeTerminal: () => void = () => {};

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
    terminal.options.fontFamily = cssFontFamily(codeFont);
    terminal.options.theme = terminalTheme(isLightColorMode(colorMode));
    requestAnimationFrame(resizeTerminal);
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
      fontFamily: cssFontFamily(codeFont),
      fontSize: 13,
      lineHeight: 1.15,
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

    const outputListener = listen<PtyOutput>("pty-output", ({ payload }) => {
      if (payload.taskId === taskId) terminalInstance.write(decodeBase64(payload.data));
    });
    const exitListener = listen<PtyExit>("pty-exit", ({ payload }) => {
      if (payload.taskId !== taskId) return;
      terminalInstance.options.cursorBlink = false;
      terminalInstance.writeln("");
      terminalInstance.writeln(
        payload.error
          ? `\x1b[31m${payload.error}\x1b[0m`
          : `\x1b[90mPi exited with code ${payload.exitCode ?? "unknown"}.\x1b[0m`,
      );
      onExit(payload.exitCode, payload.error);
    });

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

    const resizeObserver = new ResizeObserver(resize);
    resizeObserver.observe(container);
    resize();

    const dataDisposable = terminalInstance.onData((data) => {
      void invoke("write_pi_task", { taskId, data }).catch((error) => {
        terminalInstance.writeln(`\r\n\x1b[31mInput failed: ${String(error)}\x1b[0m`);
      });
    });
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
      resizeObserver.disconnect();
      dataDisposable.dispose();
      imageAddon?.dispose();
      terminalInstance.dispose();
      terminal = undefined;
      resizeTerminal = () => {};
      void outputListener.then((unlisten) => unlisten());
      void exitListener.then((unlisten) => unlisten());
    };
  });
</script>

<section class:hidden={!visible} class="terminal-pane" aria-label={`${title} terminal`}>
  <header>{title}</header>
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
  }

  .terminal-pane.hidden {
    display: none;
  }

  header {
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

  .terminal {
    min-width: 0;
    min-height: 0;
    padding: 6px 4px 2px;
  }

  .terminal :global(.xterm) {
    height: 100%;
  }
</style>
