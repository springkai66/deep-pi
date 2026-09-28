import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const page = readFileSync(new URL("../routes/+page.svelte", import.meta.url), "utf8");
const notice = readFileSync(new URL("./RuntimeUpdateNotice.svelte", import.meta.url), "utf8");

describe("runtime update startup wiring", () => {
  it("checks components after successful startup without delaying the workspace", () => {
    expect(page).toMatch(/startupPending = false;\s*if \(ready\) \{\s*\/\/[^\n]*\n\s*void checkUpdates\(\);/);
    expect(page).toContain("void checkDeepPiUpdate();");
    expect(page).not.toMatch(/setTimeout\(\(\) => \{\s*void checkUpdates\(\)/);
  });

  it("puts versioned install, snooze and skip controls in the main toolbar", () => {
    expect(page).toContain("pendingRuntimeUpdates(updates, settings, reminderNow)");
    expect(page).toContain("const timer = setTimeout(() => { reminderNow = Date.now(); }");
    expect(page).toContain("<RuntimeUpdateNotice updates={pendingUpdates}");
    expect(page).toContain("onInstall={(update) => void installRuntime(update)}");
    expect(page).toContain("onSnooze={snoozeRuntime} onSkip={skipRuntime}");
    expect(notice).toContain("{current.name} · {current.latestVersion}");
    expect(notice).toContain("onInstall(current)");
    expect(notice).toContain("onSnooze(current)");
    expect(notice).toContain("onSkip(current)");
  });
});
