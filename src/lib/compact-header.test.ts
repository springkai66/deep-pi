import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const page = readFileSync("src/routes/+page.svelte", "utf8");
const chat = readFileSync("src/lib/ChatPane.svelte", "utf8");
const tabs = readFileSync("src/lib/TaskTabs.svelte", "utf8").replace(/\r\n/g, "\n");

describe("compact session header invariants", () => {
  it("shares one tab definition and only mounts it in the active visible chat", () => {
    expect(page.match(/<TaskTabs\b/g)).toHaveLength(1);
    expect(page).toContain('tabs={inlineSessionTabs && task.id === activeTaskId ? sessionTabs : undefined}');
    expect(page).toContain('!boardView && !inspectingFile');
    expect(page).toContain('task.interactionMode === "rpc" && paneTaskIds.includes(task.id)');
    expect(page).toContain('style:grid-row={inlineSessionTabs ? "1 / -1" : undefined}');
    expect(page).not.toContain('class="crumb-task" title={activeTask?.title');
  });

  it("keeps fallback tabs out of the board and above the native DSH surface", () => {
    expect(page).toContain('view === "workspace" && !(boardView && activeAgent === "pi")');
    expect(page).toContain('{#if standaloneSessionTabs}');
    expect(page).toContain('style:grid-row={standaloneSessionTabs ? "2" : "1 / -1"}');
    expect(page.match(/workspace.querySelector\("\.dsh-placeholder"\)/g)).toHaveLength(2);
  });

  it("keeps file and diff overlays within the row below fallback tabs", () => {
    const preview = page.slice(page.indexOf('<div class="file-preview-layer"'), page.indexOf('{#if recoveryProject && recoveryModule}'));
    expect(preview).toContain('class:panel-hidden={!inspectingFile}');
    expect(preview).toContain('style:grid-row={standaloneSessionTabs ? "2" : "1 / -1"}');
    expect(preview).toContain('bind:this={fileEditor}');
    expect(preview).toContain('<GitDiffView');
  });

  it("replaces the title, suppresses empty metadata, and preserves narrow-pane actions", () => {
    expect(chat).toContain('tabs?: Snippet');
    expect(chat).toContain('<div class="header-tabs">{@render tabs()}</div>');
    expect(chat).toContain('Object.values(sessionStats.tokens).some((value) => value > 0)');
    expect(chat).toContain('(sessionStats.cost ?? 0) > 0');
    expect(chat).toContain('modelName.trim().toLowerCase() !== "unknown"');
    expect(chat).toContain('@container (max-width: 600px)');
    expect(tabs).toContain('overflow-x: auto');
    expect(tabs).toContain('.session-tab:focus-within .session-tab-close');
    expect(tabs).not.toContain('tabindex="-1"\n          class="session-tab-close"');
    expect(tabs).toContain('width: 22px;\n    height: 22px;');
  });
});
