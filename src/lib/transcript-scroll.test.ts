import { describe, expect, it } from "vitest";
import { captureTranscriptAnchor, restoreTranscriptAnchor, messageWindowStart, type TranscriptPort } from "./transcript-scroll";

function fixture() {
  const positions = new Map([["10", -30], ["11", 50]]);
  let scroll = 100;
  const port: TranscriptPort = {
    top: () => scroll,
    setTop: (value) => { scroll = value; },
    rows: () => [...positions].map(([id, top]) => ({ id, top, bottom: top + 80 })),
  };
  return { positions, port, top: () => scroll };
}

describe("transcript reading position", () => {
  it("does not evict older visible messages as new messages arrive during reading", () => {
    const pinned = messageWindowStart(500, 100, null);
    expect(messageWindowStart(501, 100, pinned)).toBe(400);
    expect(messageWindowStart(900, 100, pinned)).toBe(400);
    expect(messageWindowStart(900, 100, null)).toBe(800);
    expect(messageWindowStart(20, 100, 400)).toBe(0);
  });
  it("keeps the visible message offset when older messages are prepended", () => {
    const f = fixture();
    const anchor = captureTranscriptAnchor(f.port);
    f.positions.set("10", 970);
    f.positions.set("11", 1050);
    restoreTranscriptAnchor(f.port, anchor);
    expect(f.top()).toBe(1100);
  });
  it("does not use total transcript growth when only a later streaming message changes", () => {
    const f = fixture();
    const anchor = captureTranscriptAnchor(f.port);
    f.positions.set("12", 1500);
    restoreTranscriptAnchor(f.port, anchor);
    expect(f.top()).toBe(100);
  });
  it("leaves the current scroll alone when the selected message was removed", () => {
    const f = fixture();
    const anchor = captureTranscriptAnchor(f.port);
    f.positions.clear();
    restoreTranscriptAnchor(f.port, anchor);
    expect(f.top()).toBe(100);
    expect(captureTranscriptAnchor(f.port)).toBeNull();
  });
});
