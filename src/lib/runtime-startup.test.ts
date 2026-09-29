import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const page = readFileSync(new URL("../routes/+page.svelte", import.meta.url), "utf8");
const settings = readFileSync(new URL("./RuntimeSettings.svelte", import.meta.url), "utf8");

describe("runtime update startup wiring", () => {
  it("checks components after successful startup without delaying the workspace", () => {
    expect(page).toMatch(/startupPending = false;\s*if \(ready\) \{\s*\/\/[^\n]*\n\s*void checkUpdates\(\);/);
    expect(page).toContain("void checkDeepPiUpdate();");
    expect(page).not.toMatch(/setTimeout\(\(\) => \{\s*void checkUpdates\(\)/);
  });

  it("keeps runtime updates in settings without showing a main toolbar notice", () => {
    expect(page).not.toContain("<RuntimeUpdateNotice");
    expect(page).not.toContain("runtime-updates-visible");
    expect(page).toContain("onCheckUpdates={() => void checkUpdates(true)}");
    expect(page).toContain("onUpdateRuntime={(update) => void installRuntime(update)}");
    expect(settings).toContain("onCheckUpdates");
    expect(settings).toContain("{t(\"检查组件更新\")}");
    expect(settings).toContain("{t(\"可更新 · {version}\", { version: update.latestVersion ?? \"\" })}");
    expect(settings).toContain("onclick={() => onUpdateRuntime(update)}");
  });
});
