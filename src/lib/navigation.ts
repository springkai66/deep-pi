export type NavigationCommand =
  | "tasks" | "files" | "settings" | "sidebar" | "composer" | "workspacePi" | "workspaceDsh";

export function matchesSearch(value: string, query: string): boolean {
  const needle = Array.from(query.trim().toLocaleLowerCase());
  if (!needle.length) return true;
  let index = 0;
  for (const character of value.toLocaleLowerCase()) {
    if (character === needle[index]) index++;
    if (index === needle.length) return true;
  }
  return false;
}

export function shortcutCommand(event: ShortcutEvent): NavigationCommand | null {
  if (event.repeat) return null;
  const command = matchHostShortcut(event);
  return command === "save" || command === "saveAs" ? null : command;
}
import { matchHostShortcut, type ShortcutEvent } from "./shortcuts";
