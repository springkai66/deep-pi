/**
 * 从剪贴板事件数据里取出图片文件。
 *
 * 不同来源的图片可能出现在 `items`（截图工具常见）或 `files`（文件复制常见），
 * WebView2/Chromium 的行为不一致；只读 `files` 会静默丢图（用户看到「粘贴后没有附件」）。
 * 这里合并两处并按 name+size+type 去重，保证截图与图片文件都能收进附件。
 */
export interface ClipboardItemLike {
  kind: string;
  type: string;
  getAsFile?: () => File | null;
}

export interface ClipboardDataLike {
  items?: ArrayLike<ClipboardItemLike> | null;
  files?: ArrayLike<File> | null;
}

function isImage(file: File | null | undefined): file is File {
  return Boolean(file && typeof file.type === "string" && file.type.startsWith("image/"));
}

export function clipboardImageFiles(data: ClipboardDataLike | null | undefined): File[] {
  if (!data) return [];
  const files: File[] = [];
  const seen = new Set<string>();
  const push = (file: File | null | undefined) => {
    if (!isImage(file)) return;
    const key = `${file.name}\u0000${file.size}\u0000${file.type}`;
    if (seen.has(key)) return;
    seen.add(key);
    files.push(file);
  };
  const items = data.items;
  if (items) {
    for (let index = 0; index < items.length; index += 1) {
      const item = items[index];
      if (!item || item.kind !== "file" || !item.type.startsWith("image/")) continue;
      push(item.getAsFile?.());
    }
  }
  const list = data.files;
  if (list) {
    for (let index = 0; index < list.length; index += 1) push(list[index]);
  }
  return files;
}
