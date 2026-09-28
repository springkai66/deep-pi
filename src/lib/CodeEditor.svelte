<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { EditorState } from "@codemirror/state";
  import { EditorView, keymap, lineNumbers, highlightActiveLineGutter, drawSelection } from "@codemirror/view";
  import { defaultKeymap, historyKeymap } from "@codemirror/commands";
  import { bracketMatching, defaultHighlightStyle, syntaxHighlighting, LanguageDescription } from "@codemirror/language";
  import { languages } from "@codemirror/language-data";
  import { editorLanguageConfig, editorViewConfig, editorReadOnlyConfig } from "./editor-document";
  import type { FileDocument } from "./file-workspace";
  import { shortcutKeymap } from "./shortcuts";

  let { document, onUpdate, onScroll, onSave, line, column, visible = true }: {
    document: FileDocument; onUpdate: (id: string, state: EditorState) => void;
    onScroll: (id: string, scroll: { top: number; left: number }) => void;
    onSave: () => void; line?: number; column?: number; visible?: boolean;
  } = $props();
  let host: HTMLDivElement;
  let view = $state.raw<EditorView | null>(null);
  let appliedLocation = "";
  let appliedLock: boolean | null = null;
  onMount(() => {
    if (!document.state) return;
    const id = document.id;
    const editor = new EditorView({ parent: host, state: document.state });
    view = editor;
    editor.dispatch({ effects: editorViewConfig.reconfigure([
      lineNumbers(), highlightActiveLineGutter(), drawSelection(), bracketMatching(),
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      keymap.of([{ key: shortcutKeymap("save"), run: (current) => { if (current.composing || current.state.readOnly) return false; onSave(); return true; } }, ...defaultKeymap, ...historyKeymap]),
      EditorView.contentAttributes.of({ "aria-label": document.path, spellcheck: "false" }),
      EditorView.updateListener.of((update) => {
        if (update.docChanged || update.selectionSet || update.transactions.some((transaction) => transaction.effects.length)) onUpdate(id, update.state);
      }),
      EditorView.domEventHandlers({ scroll: (_event, current) => {
        onScroll(id, { top: current.scrollDOM.scrollTop, left: current.scrollDOM.scrollLeft });
      } }),
      EditorView.theme({
        "&": { height: "100%", color: "var(--text)", backgroundColor: "var(--page-bg)" },
        ".cm-scroller": { overflow: "auto", fontFamily: "var(--code-font)", fontSize: "13px" },
        ".cm-content": { caretColor: "var(--text)", padding: "12px 0" },
        ".cm-gutters": { backgroundColor: "var(--surface)", color: "var(--text-muted)", borderRight: "1px solid var(--border)" },
        ".cm-activeLineGutter": { backgroundColor: "var(--surface-hover)" },
        ".cm-cursor": { borderLeftColor: "var(--text)" },
        "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": { backgroundColor: "var(--surface-hover)" },
      }),
    ]) });
    editor.scrollDOM.scrollTop = document.scroll.top;
    editor.scrollDOM.scrollLeft = document.scroll.left;
    let alive = true;
    const language = LanguageDescription.matchFilename(languages, document.path);
    if (language) void language.load().then((support) => {
      if (alive) editor.dispatch({ effects: editorLanguageConfig.reconfigure(support) });
    }).catch(() => {});
    return () => {
      alive = false;
      onScroll(id, { top: editor.scrollDOM.scrollTop, left: editor.scrollDOM.scrollLeft });
      editor.destroy();
      view = null;
    };
  });
  $effect(() => {
    const current = view;
    if (current && visible) untrack(() => current.requestMeasure());
  });
  $effect(() => {
    const current = view;
    const locked = document.locked;
    if (!current || appliedLock === locked) return;
    appliedLock = locked;
    untrack(() => current.dispatch({ effects: editorReadOnlyConfig.reconfigure([
      EditorState.readOnly.of(locked), EditorView.editable.of(!locked),
    ]) }));
  });
  $effect(() => {
    const current = view;
    const state = document.state;
    if (current && state && current.state !== state) {
      untrack(() => current.setState(state));
    }
  });
  $effect(() => {
    const current = view;
    const row = line;
    const col = column ?? 1;
    const location = `${document.id}:${row}:${col}`;
    if (!current || !row || location === appliedLocation) return;
    appliedLocation = location;
    untrack(() => {
      const target = current.state.doc.line(Math.max(1, Math.min(row, current.state.doc.lines)));
      const prefix = Array.from(target.text).slice(0, Math.max(0, col - 1)).join("");
      const anchor = target.from + prefix.length;
      current.dispatch({ selection: { anchor }, effects: EditorView.scrollIntoView(anchor, { y: "center" }) });
      current.focus();
    });
  });
</script>

<div class="code-editor" bind:this={host}></div>

<style>
  .code-editor { height: 100%; min-height: 0; min-width: 0; overflow: hidden; }
</style>
