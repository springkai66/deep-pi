import { describe, expect, it, vi } from "vitest";
import { clipboardImageFiles, type ClipboardDataLike } from "./clipboard-images";

const file = (name: string, size: number, type: string): File => ({ name, size, type }) as File;

describe("clipboard image extraction", () => {
  it("reads screenshots that only appear in items", () => {
    const data: ClipboardDataLike = {
      items: [{ kind: "file", type: "image/png", getAsFile: () => file("image.png", 100, "image/png") }],
      files: [] as unknown as ArrayLike<File>,
    };
    const result = clipboardImageFiles(data);
    expect(result).toHaveLength(1);
    expect(result[0]?.type).toBe("image/png");
  });

  it("reads copied image files and keeps text items out", () => {
    const getAsFile = vi.fn(() => file("text.txt", 3, "text/plain"));
    const data: ClipboardDataLike = {
      items: [{ kind: "string", type: "text/plain", getAsFile }, { kind: "file", type: "text/plain", getAsFile }],
      files: [file("photo.jpg", 2048, "image/jpeg")],
    };
    const result = clipboardImageFiles(data);
    expect(result.map((entry) => entry.name)).toEqual(["photo.jpg"]);
    // 非图片项不调用 getAsFile
    expect(getAsFile).not.toHaveBeenCalled();
  });

  it("deduplicates the same image reported by both items and files", () => {
    const same = file("image.png", 100, "image/png");
    const data: ClipboardDataLike = {
      items: [{ kind: "file", type: "image/png", getAsFile: () => same }],
      files: [same],
    };
    expect(clipboardImageFiles(data)).toHaveLength(1);
  });

  it("tolerates missing or empty clipboard data", () => {
    expect(clipboardImageFiles(null)).toEqual([]);
    expect(clipboardImageFiles(undefined)).toEqual([]);
    expect(clipboardImageFiles({})).toEqual([]);
    expect(clipboardImageFiles({ items: [], files: [] })).toEqual([]);
  });
});
