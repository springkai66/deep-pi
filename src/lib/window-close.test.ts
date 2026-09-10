import { describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { createWindowCloseHandler, type WindowClosePorts } from "./window-close";

it("keeps the window alive during an index transaction and allows retry after it finishes", async () => {
  let writing = true;
  const destroy = vi.fn(async () => {});
  const error = vi.fn();
  const handler = createWindowCloseHandler(ports({
    blocked: () => writing,
    blockedReason: () => "Git 索引正在写入，请完成后再关闭窗口",
    destroy, error,
  }));
  await handler({ preventDefault() {} });
  expect(destroy).not.toHaveBeenCalled();
  expect(error.mock.calls[0][0].message).toContain("Git");
  writing = false;
  await handler({ preventDefault() {} });
  expect(destroy).toHaveBeenCalledTimes(1);
});

function ports(overrides: Partial<WindowClosePorts> = {}): WindowClosePorts {
  return {
    behavior: () => "exit", blocked: () => false, hasActiveTasks: () => false,
    choose: async () => "exit", confirm: async () => true,
    remember: () => {}, minimize: async () => {}, stopPi: async () => {},
    stopDsh: async () => {}, destroy: async () => {}, error: () => {},
    ...overrides,
  };
}

describe("window close", () => {
  it("holds the exit lease through task shutdown and releases it after destroy fails", async () => {
    const calls: string[] = [];
    const release = vi.fn(() => { calls.push("release"); });
    const handler = createWindowCloseHandler(ports({
      prepareExit: async () => release,
      stopPi: async () => { expect(release).not.toHaveBeenCalled(); calls.push("pi"); },
      stopDsh: async () => { expect(release).not.toHaveBeenCalled(); calls.push("dsh"); },
      destroy: async () => { expect(release).not.toHaveBeenCalled(); throw new Error("destroy failed"); },
      error: () => { calls.push("error"); },
    }));
    await handler({ preventDefault() {} });
    expect(calls).toEqual(["pi", "dsh", "error", "release"]);
    expect(release).toHaveBeenCalledOnce();
  });

  it("releases the exit lease if a new operation blocks shutdown after preparation", async () => {
    let blocked = false;
    const release = vi.fn();
    const stopPi = vi.fn();
    const handler = createWindowCloseHandler(ports({
      blocked: () => blocked,
      prepareExit: async () => { blocked = true; return release; },
      stopPi,
    }));
    await handler({ preventDefault() {} });
    expect(stopPi).not.toHaveBeenCalled();
    expect(release).toHaveBeenCalledOnce();
  });

  it("releases an exit lease after either task manager fails to stop", async () => {
    for (const failure of ["pi", "dsh"]) {
      const release = vi.fn();
      const destroy = vi.fn();
      const handler = createWindowCloseHandler(ports({
        prepareExit: async () => release,
        stopPi: async () => { if (failure === "pi") throw new Error("pi failed"); },
        stopDsh: async () => { if (failure === "dsh") throw new Error("dsh failed"); },
        destroy,
      }));
      await handler({ preventDefault() {} });
      expect(destroy).not.toHaveBeenCalled();
      expect(release).toHaveBeenCalledOnce();
    }
  });

  it("waits for exit preparation before stopping tasks and allows retry after save failure", async () => {
    const calls: string[] = [];
    const prepareExit = vi.fn()
      .mockRejectedValueOnce("settings save failed")
      .mockImplementationOnce(async () => { calls.push("prepared"); return true; });
    const error = vi.fn();
    const handler = createWindowCloseHandler(ports({
      prepareExit,
      stopPi: async () => { calls.push("pi"); },
      stopDsh: async () => { calls.push("dsh"); },
      destroy: async () => { calls.push("destroy"); },
      error,
    }));
    await handler({ preventDefault() {} });
    expect(calls).toEqual([]);
    expect(error).toHaveBeenCalledWith("settings save failed");
    await handler({ preventDefault() {} });
    expect(calls).toEqual(["prepared", "pi", "dsh", "destroy"]);
  });

  it("keeps tasks alive when exit preparation is cancelled", async () => {
    const stopPi = vi.fn();
    const destroy = vi.fn();
    const handler = createWindowCloseHandler(ports({
      prepareExit: async () => false, stopPi, destroy,
    }));
    await handler({ preventDefault() {} });
    expect(stopPi).not.toHaveBeenCalled();
    expect(destroy).not.toHaveBeenCalled();
  });

  it("rechecks operation protection after asynchronous exit preparation", async () => {
    let blocked = false;
    const destroy = vi.fn();
    const stopPi = vi.fn();
    const error = vi.fn();
    const handler = createWindowCloseHandler(ports({
      blocked: () => blocked,
      prepareExit: async () => { blocked = true; return true; },
      stopPi, destroy, error,
    }));
    await handler({ preventDefault() {} });
    expect(stopPi).not.toHaveBeenCalled();
    expect(destroy).not.toHaveBeenCalled();
    expect(error).toHaveBeenCalledOnce();
  });

  it("does not prepare an exit when minimizing", async () => {
    const prepareExit = vi.fn();
    const handler = createWindowCloseHandler(ports({ behavior: () => "minimize", prepareExit }));
    await handler({ preventDefault() {} });
    expect(prepareExit).not.toHaveBeenCalled();
  });

  it("authorizes destroy and minimize only for the main Webview", () => {
    const capability = JSON.parse(readFileSync("src-tauri/capabilities/window-lifecycle.json", "utf8"));
    expect(capability.webviews).toEqual(["main"]);
    expect(capability.windows ?? []).toEqual([]);
    expect(capability.remote).toBeUndefined();
    expect(capability.permissions).toEqual(["core:window:allow-destroy", "core:window:allow-minimize"]);
  });

  it("prevents every repeated close event while confirmation is pending", async () => {
    let answer!: (value: boolean) => void;
    const confirm = vi.fn(() => new Promise<boolean>((resolve) => { answer = resolve; }));
    const calls: string[] = [];
    const handler = createWindowCloseHandler(ports({
      hasActiveTasks: () => true, confirm,
      stopPi: async () => { calls.push("pi"); },
      stopDsh: async () => { calls.push("dsh"); },
      destroy: async () => { calls.push("destroy"); },
    }));
    const preventDefault = vi.fn();
    const first = handler({ preventDefault });
    await handler({ preventDefault });
    expect(preventDefault).toHaveBeenCalledTimes(2);
    expect(confirm).toHaveBeenCalledTimes(1);
    answer(true);
    await first;
    expect(calls).toEqual(["pi", "dsh", "destroy"]);
  });

  it("keeps the window open on cancellation and permits the next close", async () => {
    const destroy = vi.fn(async () => {});
    const confirm = vi.fn().mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    const handler = createWindowCloseHandler(ports({ hasActiveTasks: () => true, confirm, destroy }));
    await handler({ preventDefault() {} });
    expect(destroy).not.toHaveBeenCalled();
    await handler({ preventDefault() {} });
    expect(destroy).toHaveBeenCalledOnce();
  });

  it("exits after the initial close choice and active-task confirmation", async () => {
    const calls: string[] = [];
    const handler = createWindowCloseHandler(ports({
      behavior: () => "ask",
      choose: async () => { calls.push("choice"); return "exit"; },
      remember: (choice) => { calls.push(`remember:${choice}`); },
      hasActiveTasks: () => true,
      confirm: async () => { calls.push("confirm"); return true; },
      stopPi: async () => { calls.push("pi"); },
      stopDsh: async () => { calls.push("dsh"); },
      destroy: async () => { calls.push("destroy"); },
    }));
    await handler({ preventDefault() {} });
    expect(calls).toEqual(["choice", "remember:exit", "confirm", "pi", "dsh", "destroy"]);
  });

  it("catches destroy rejection and permits a new close attempt", async () => {
    const destroy = vi.fn().mockRejectedValueOnce("window.destroy not allowed").mockResolvedValueOnce(undefined);
    const error = vi.fn();
    const handler = createWindowCloseHandler(ports({ destroy, error }));
    await handler({ preventDefault() {} });
    expect(error).toHaveBeenCalledWith("window.destroy not allowed");
    await handler({ preventDefault() {} });
    expect(destroy).toHaveBeenCalledTimes(2);
  });

  it("reports minimize failure and allows retry without stopping tasks", async () => {
    const error = vi.fn();
    const stopPi = vi.fn();
    const minimize = vi.fn().mockRejectedValueOnce("permission denied").mockResolvedValueOnce(undefined);
    const handler = createWindowCloseHandler(ports({ behavior: () => "minimize", minimize, stopPi, error }));
    await handler({ preventDefault() {} });
    await handler({ preventDefault() {} });
    expect(error).toHaveBeenCalledWith("permission denied");
    expect(minimize).toHaveBeenCalledTimes(2);
    expect(stopPi).not.toHaveBeenCalled();
  });

  it("does not destroy on stop failure and retries on the next request", async () => {
    const destroy = vi.fn(async () => {});
    const error = vi.fn();
    const stopPi = vi.fn().mockRejectedValueOnce("stop failed").mockResolvedValueOnce(undefined);
    const handler = createWindowCloseHandler(ports({ destroy, stopPi, error }));
    await handler({ preventDefault() {} });
    expect(destroy).not.toHaveBeenCalled();
    expect(error).toHaveBeenCalledWith("stop failed");
    await handler({ preventDefault() {} });
    expect(destroy).toHaveBeenCalledOnce();
  });
});
