import { describe, expect, it } from "vitest";
import { createPetTaskBubbles, type PetTaskBubblesState } from "./pet-task-bubbles";
import { observePetTaskHover, type PetTaskHover } from "./pet-task-hover";
import type { Task } from "./task";

function runningTask(id: string, agent: "pi" | "dsh" = "pi", status: Task["status"] = "running"): Task {
  return {
    id, title: `任务 ${id}`, agent, status, archivedAt: null,
  } as Task;
}

const flush = async () => {
  for (let i = 0; i < 10; i += 1) await Promise.resolve();
};

describe("pet hover startup", () => {
  it("opens a ring with the running Pi title when hover happens before popup listener registration", async () => {
    const pi = runningTask("pi-running");
    const dsh = runningTask("dsh-running", "dsh");
    const states: PetTaskBubblesState[] = [];
    const shown: boolean[] = [];
    const bubbles = createPetTaskBubbles({
      load: async () => [
        pi, runningTask("pi-waiting", "pi", "waiting"),
        runningTask("pi-completed", "pi", "completed"),
        { ...runningTask("pi-archived"), archivedAt: 1 }, dsh,
      ],
      resize: async (open) => { shown.push(open); },
      action: async () => {},
      changed: (state) => { states.push(state); },
    });
    bubbles.setAgent("pi");
    let latest: PetTaskHover = { revision: 0, hovered: false };
    let listener: ((state: PetTaskHover) => void) | undefined;
    let register: (() => void) | undefined;
    const stop = observePetTaskHover(
      (onHover) => new Promise((resolve) => {
        register = () => { listener = onHover; resolve(() => { listener = undefined; }); };
      }),
      async () => latest,
      (hovered) => { if (hovered) void bubbles.enter(); else bubbles.leave(); },
    );
    await flush(); // An old unhovered snapshot can resolve before registration.
    latest = { revision: 1, hovered: true };
    listener?.(latest); // No recipient yet; native hover state still records the event.
    register?.();
    await flush();
    expect(shown).toContain(true);
    expect(states.at(-1)?.open).toBe(true);
    expect(states.at(-1)?.tasks.map((task) => task.title)).toEqual([pi.title]);
    bubbles.setAgent("dsh");
    await flush();
    expect(states.at(-1)?.tasks.map((task) => task.title)).toEqual([dsh.title]);
    stop();
    bubbles.dispose();
  });

  it("ignores a stale snapshot and disposes the listener", async () => {
    let listener: ((state: PetTaskHover) => void) | undefined;
    let releaseSnapshot: ((state: PetTaskHover) => void) | undefined;
    let unsubscribed = false;
    const changes: boolean[] = [];
    const stop = observePetTaskHover(
      async (onHover) => { listener = onHover; return () => { unsubscribed = true; }; },
      () => new Promise((resolve) => { releaseSnapshot = resolve; }),
      (hovered) => { changes.push(hovered); },
    );
    await flush();
    listener?.({ revision: 2, hovered: true });
    releaseSnapshot?.({ revision: 1, hovered: false });
    await flush();
    expect(changes).toEqual([true]);
    stop();
    await flush();
    expect(unsubscribed).toBe(true);
  });
});
