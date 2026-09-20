import { describe, expect, it } from "vitest";
import { hasQueuedImages, rekeyQueuedImages, takeQueuedImages, type QueuedImageEntry } from "./queued-images";

const entries = (): QueuedImageEntry<string>[] => [
  { mode: "steer", text: "first", images: ["a"] },
  { mode: "followUp", text: "second", images: ["b"] },
  { mode: "steer", text: "first", images: ["c"] },
];

describe("queued image memory", () => {
  it("detects remembered images by mode and text", () => {
    expect(hasQueuedImages(entries(), "steer", "first")).toBe(true);
    expect(hasQueuedImages(entries(), "followUp", "first")).toBe(false);
    expect(hasQueuedImages(entries(), "steer", "missing")).toBe(false);
  });

  it("consumes duplicate texts in order and leaves the rest untouched", () => {
    const first = takeQueuedImages(entries(), "steer", "first");
    expect(first.images).toEqual(["a"]);
    expect(first.entries).toHaveLength(2);
    const second = takeQueuedImages(first.entries, "steer", "first");
    expect(second.images).toEqual(["c"]);
    expect(second.entries).toHaveLength(1);
    const missing = takeQueuedImages(second.entries, "steer", "first");
    expect(missing.images).toEqual([]);
    expect(missing.entries).toHaveLength(1);
  });

  it("does not mutate the input array when taking", () => {
    const source = entries();
    takeQueuedImages(source, "steer", "first");
    expect(source).toHaveLength(3);
  });

  it("rekeys only the matching entry when moving between queues", () => {
    const moved = rekeyQueuedImages(entries(), "steer", "followUp", "first");
    expect(moved[0]).toEqual({ mode: "followUp", text: "first", images: ["a"] });
    expect(moved[1]).toEqual({ mode: "followUp", text: "second", images: ["b"] });
    expect(moved[2]).toEqual({ mode: "steer", text: "first", images: ["c"] });
    // 原数组不被修改
    expect(entries()[0].mode).toBe("steer");
  });
});
