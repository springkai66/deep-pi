export function createLatestSearch<T>(
  execute: (id: string) => Promise<T>,
  cancelRemote: (id: string) => Promise<unknown>,
) {
  let generation = 0;
  let active: string | null = null;
  let queue = Promise.resolve();
  function cancel() {
    generation++;
    if (active) void cancelRemote(active).catch(() => {});
  }
  return {
    cancel,
    run(id: string, onResult: (value: T) => void, onError: (cause: unknown) => void) {
      cancel();
      const current = generation;
      const pending = queue.then(async () => {
        if (current !== generation) return;
        active = id;
        try {
          const result = await execute(id);
          if (current === generation) onResult(result);
        } catch (cause) {
          if (current === generation) onError(cause);
        } finally {
          if (active === id) active = null;
        }
      });
      queue = pending.catch(() => {});
      return pending;
    },
  };
}
export function sourceOffset(text: string, line: number, column: number): number {
  let offset = 0;
  for (let row = 1; row < line; row++) {
    const newline = text.indexOf("\n", offset);
    if (newline < 0) return text.length;
    offset = newline + 1;
  }
  for (let col = 1; col < column && offset < text.length; col++) {
    if (text[offset] === "\r" || text[offset] === "\n") break;
    offset += text.codePointAt(offset)! > 0xffff ? 2 : 1;
  }
  return offset;
}
export interface SearchMatch {
  path: string;
  line: number;
  column: number;
  text: string;
}

export function groupSearchMatches(matches: SearchMatch[]): { path: string; matches: SearchMatch[] }[] {
  const groups = new Map<string, SearchMatch[]>();
  for (const match of matches) {
    const group = groups.get(match.path);
    if (group) group.push(match);
    else groups.set(match.path, [match]);
  }
  return Array.from(groups, ([path, matches]) => ({ path, matches }));
}
