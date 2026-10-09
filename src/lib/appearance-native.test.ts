import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { DEFAULT_APP_SETTINGS } from "./settings";
import { TRANSPARENT_THEME } from "./theme";

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
const glass = { ...DEFAULT_APP_SETTINGS, theme: TRANSPARENT_THEME.id };

it("keeps the main canvas transparent across slider changes without native material requests", async () => {
  const element = root();
  const { applyAppearance } = await import("./appearance");
  applyAppearance(glass);
  expect(element.dataset.windowMaterial).toBe("transparent");
  applyAppearance({ ...glass, themeTransparency: 100 });
  expect(element.dataset.windowMaterial).toBe("transparent");
  expect(native.invoke).not.toHaveBeenCalled();
});

it("switches immediately back to an opaque theme without native material requests", async () => {
  const element = root();
  const { applyAppearance } = await import("./appearance");
  applyAppearance(glass);
  applyAppearance(DEFAULT_APP_SETTINGS);
  expect(element.dataset.windowMaterial).toBe("none");
  expect(native.invoke).not.toHaveBeenCalled();
});
