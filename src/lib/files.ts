import { matchesSearch } from "./navigation";

export interface FileEntry {
  path: string;
  name: string;
  isDirectory: boolean;
  size: number;
}

export interface FileIndex {
  entries: FileEntry[];
  truncated: boolean;
  unreadableDirectories: string[];
}

export interface FilePreview {
  path: string;
  content: string;
  size: number;
  version: string;
}

export function parentPath(path: string): string {
  const slash = path.lastIndexOf("/");
  return slash < 0 ? "" : path.slice(0, slash);
}

export function visibleFiles(entries: FileEntry[], query: string, expanded: ReadonlySet<string>): FileEntry[] {
  const searching = query.trim().length > 0;
  const included = new Set<string>();
  if (searching) {
    for (const entry of entries) {
      if (!matchesSearch(entry.path, query)) continue;
      included.add(entry.path);
      let parent = parentPath(entry.path);
      while (parent && !included.has(parent)) {
        included.add(parent);
        parent = parentPath(parent);
      }
    }
  }
  const children = new Map<string, FileEntry[]>();
  for (const entry of entries) {
    if (searching && !included.has(entry.path)) continue;
    const parent = parentPath(entry.path);
    const list = children.get(parent);
    if (list) list.push(entry);
    else children.set(parent, [entry]);
  }
  const result: FileEntry[] = [];
  function visit(parent: string) {
    const list = children.get(parent) ?? [];
    list.sort((a, b) => Number(b.isDirectory) - Number(a.isDirectory) || a.name.localeCompare(b.name));
    for (const entry of list) {
      result.push(entry);
      if (entry.isDirectory && (searching || expanded.has(entry.path))) visit(entry.path);
    }
  }
  visit("");
  return result;
}
