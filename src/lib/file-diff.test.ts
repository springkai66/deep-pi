import { describe, expect, it } from "vitest";
import { Chunk } from "@codemirror/merge";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { createDiffConfig, diffMetadata, nextDiffIndex } from "./file-diff";
import { shortcutKeymap } from "./shortcuts";

describe("read-only file diff", () => {
  it("installs shared navigation chords in both real editor states and respects composition", () => {
    const directions: number[] = [];
    const config = createDiffConfig("old\n", "new\n", (direction) => { directions.push(direction); });
    for (const side of [config.a, config.b]) {
      const state = EditorState.create(side);
      const bindings = state.facet(keymap).flat();
      const next = bindings.find((binding) => binding.key === shortcutKeymap("nextDiff"))!;
      const previous = bindings.find((binding) => binding.key === shortcutKeymap("previousDiff"))!;
      expect(next.run?.({ composing: true } as EditorView)).toBe(false);
      expect(next.run?.({ composing: false } as EditorView)).toBe(true);
      expect(previous.run?.({ composing: false } as EditorView)).toBe(true);
    }
    expect(directions).toEqual([1, -1, 1, -1]);
  });
  it("protects both sides from edits and omits revert controls", () => {
    const config = createDiffConfig("old\n", "new\n");
    for (const side of [config.a, config.b]) {
      const state = EditorState.create(side);
      expect(state.readOnly).toBe(true);
      expect(state.facet(EditorView.editable)).toBe(false);
      const attemptedWrite = state.update({ changes: { from: 0, to: state.doc.length, insert: "replacement" } });
      expect(attemptedWrite.state.doc.eq(state.doc)).toBe(true);
    }
    expect(config.revertControls).toBeUndefined();
    expect(config.highlightChanges).toBe(true);
    expect(config.diffConfig?.scanLimit).toBe(500);
    expect(config.diffConfig?.timeout).toBe(100);
  });
  it("uses the real diff engine for additions, deletions and non-BMP Unicode", () => {
    for (const [a, b] of [["", "new\n"], ["old\n", ""], ["中文𐐀 old\n", "中文𐐀 new\n"]]) {
      const config = createDiffConfig(a, b);
      const stateA = EditorState.create(config.a);
      const stateB = EditorState.create(config.b);
      const chunks = Chunk.build(stateA.doc, stateB.doc, config.diffConfig);
      expect(chunks.length).toBeGreaterThan(0);
      for (const chunk of chunks) {
        expect(chunk.endA).toBeLessThanOrEqual(stateA.doc.length);
        expect(chunk.endB).toBeLessThanOrEqual(stateB.doc.length);
      }
    }
  });
  it("reports newline-only changes rather than claiming the files are identical", () => {
    const config = createDiffConfig("a\r\nb\r\n", "a\nb\n");
    expect(EditorState.create(config.a).doc.toString()).toBe("a\nb\n");
    const summary = diffMetadata("a\r\nb\r\n", "a\nb\n");
    expect(summary.equal).toBe(false);
    expect(summary.newlineOnly).toBe(true);
    expect(summary.sourceNewline).toBe("CRLF");
    expect(summary.draftNewline).toBe("LF");
    expect(diffMetadata("a\rb\nc", "a\nb\rc").newlineOnly).toBe(true);
  });
  it("does not hide BOM changes or whitespace-only edits", () => {
    expect(diffMetadata("\uFEFFa\n", "a\n").sourceBom).toBe(true);
    expect(diffMetadata("\uFEFFa\n", "a\n").draftBom).toBe(false);
    expect(diffMetadata("\uFEFFa\n", "a\n").newlineOnly).toBe(false);
    const config = createDiffConfig("a \n", "a\n");
    expect(Chunk.build(EditorState.create(config.a).doc, EditorState.create(config.b).doc).length).toBeGreaterThan(0);
    expect(diffMetadata("a\n", "a\n").equal).toBe(true);
  });
  it("wraps explicit difference navigation and handles zero or one difference", () => {
    expect(nextDiffIndex(-1, 3, 1)).toBe(0);
    expect(nextDiffIndex(-1, 3, -1)).toBe(2);
    expect(nextDiffIndex(2, 3, 1)).toBe(0);
    expect(nextDiffIndex(0, 3, -1)).toBe(2);
    expect(nextDiffIndex(0, 1, 1)).toBe(0);
    expect(nextDiffIndex(-1, 0, 1)).toBe(-1);
  });
  it("handles a large repeated document without dropping unchanged sections", () => {
    const source = "unchanged line\n".repeat(20_000);
    const draft = source.slice(0, source.length / 2) + "新增差异\n" + source.slice(source.length / 2);
    const config = createDiffConfig(source, draft);
    const a = EditorState.create(config.a);
    const b = EditorState.create(config.b);
    const chunks = Chunk.build(a.doc, b.doc, config.diffConfig);
    expect(a.doc.toString()).toBe(source);
    expect(b.doc.toString()).toBe(draft);
    expect(chunks.length).toBeGreaterThan(0);
    expect(chunks[0].fromA).toBeGreaterThan(0);
    expect(chunks[chunks.length - 1].endB).toBeLessThan(b.doc.length);
  });
});
