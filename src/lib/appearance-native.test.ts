import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { DEFAULT_APP_SETTINGS } from "./settings";
import { FROSTED_GLASS_THEME } from "./theme";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true, invoke: native.invoke }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ label: "main" }) }));

beforeEach(() => {
  vi.resetModules();
  native.invoke.mockReset();
  vi.stubGlobal("navigator", { userAgent: "Windows" });
});
afterEach(() => vi.unstubAllGlobals());

function root() {
  const element = { dataset: {} as Record<string, string>, lang: "", style: { setProperty() {} } };
  vi.stubGlobal("document", { documentElement: element });
  return element;
}
const glass = { ...DEFAULT_APP_SETTINGS, theme: FROSTED_GLASS_THEME.id };

it("uses the native fallback without making the glass shell opaque on later slider changes", async () => {
  const element = root();
  native.invoke.mockResolvedValue({ material: "transparent", fallback: "system_transparency_disabled" });
  const { applyAppearance } = await import("./appearance");
  applyAppearance(glass);
  await vi.waitFor(() => expect(element.dataset.windowMaterial).toBe("transparent"));
  expect(element.dataset.nativeMaterialFallback).toBe("system_transparency_disabled");
  applyAppearance({ ...glass, themeTransparency: 100 });
  expect(element.dataset.windowMaterial).toBe("transparent");
  expect(native.invoke).toHaveBeenCalledTimes(1);
});

it("ignores a stale glass response after switching back to an opaque theme", async () => {
  const element = root();
  let resolveGlass!: (value: { material: string; fallback: string }) => void;
  native.invoke.mockReturnValueOnce(new Promise(resolve => { resolveGlass = resolve; }));
  native.invoke.mockResolvedValueOnce({ material: "none", fallback: null });
  const { applyAppearance } = await import("./appearance");
  applyAppearance(glass);
  applyAppearance(DEFAULT_APP_SETTINGS);
  resolveGlass({ material: "transparent", fallback: "system_transparency_disabled" });
  await Promise.resolve();
  expect(element.dataset.windowMaterial).toBe("none");
  expect(element.dataset.nativeMaterialFallback).toBeUndefined();
});
