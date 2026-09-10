import { describe, expect, it, vi } from "vitest";
import { loadAppStartup, type StartupPorts } from "./startup";
import { DEFAULT_APP_SETTINGS } from "./settings";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function fixture(overrides: Partial<StartupPorts> = {}) {
  return {
    settings: vi.fn(async () => ({ ...DEFAULT_APP_SETTINGS })),
    tasks: vi.fn(async () => []), projects: vi.fn(async () => []),
    runtimes: vi.fn(async () => []),
    applySettings: vi.fn(), applyWorkspace: vi.fn(), applyRuntimes: vi.fn(),
    error: vi.fn(), ...overrides,
  } satisfies StartupPorts;
}

describe("application startup", () => {
  it("makes the workspace ready without waiting for runtime probes", async () => {
    const runtime = deferred<[]>();
    const ports = fixture({ runtimes: () => runtime.promise });
    const startup = loadAppStartup(ports);
    expect(await startup.ready).toBe(true);
    expect(ports.applySettings).toHaveBeenCalledOnce();
    expect(ports.applyWorkspace).toHaveBeenCalledOnce();
    expect(ports.applyRuntimes).not.toHaveBeenCalled();
    runtime.resolve([]);
    await startup.background;
    expect(ports.applyRuntimes).toHaveBeenCalledOnce();
  });

  it("reads stored projects concurrently with stored tasks", async () => {
    const tasks = deferred<[]>();
    const ports = fixture({ tasks: () => tasks.promise });
    const startup = loadAppStartup(ports);
    await Promise.resolve();
    expect(ports.projects).toHaveBeenCalledOnce();
    expect(ports.applySettings).toHaveBeenCalledOnce();
    tasks.resolve([]);
    expect(await startup.ready).toBe(true);
    expect(ports.projects).toHaveBeenCalledOnce();
    await startup.background;
  });

  it("does not discard usable settings or tasks when a runtime probe fails", async () => {
    const ports = fixture({ runtimes: async () => { throw new Error("probe unavailable"); } });
    const startup = loadAppStartup(ports);
    expect(await startup.ready).toBe(true);
    await startup.background;
    expect(ports.applyWorkspace).toHaveBeenCalledOnce();
    expect(ports.error).toHaveBeenCalledWith("runtime", expect.any(Error));
  });

  it("does not publish a partial workspace when reading settings fails", async () => {
    const ports = fixture({ settings: async () => { throw new Error("settings unavailable"); } });
    const startup = loadAppStartup(ports);
    expect(await startup.ready).toBe(false);
    expect(ports.applyWorkspace).not.toHaveBeenCalled();
    expect(ports.error).toHaveBeenCalledWith("workspace", expect.any(Error));
    await startup.background;
  });

  it("ignores delayed results and errors after disposal", async () => {
    const settings = deferred<typeof DEFAULT_APP_SETTINGS>();
    const runtime = deferred<[]>();
    const ports = fixture({ settings: () => settings.promise, runtimes: () => runtime.promise });
    const startup = loadAppStartup(ports);
    startup.dispose();
    settings.resolve({ ...DEFAULT_APP_SETTINGS });
    runtime.reject(new Error("late failure"));
    expect(await startup.ready).toBe(false);
    await startup.background;
    expect(ports.applySettings).not.toHaveBeenCalled();
    expect(ports.applyWorkspace).not.toHaveBeenCalled();
    expect(ports.error).not.toHaveBeenCalled();
  });
});
