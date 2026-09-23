import { t } from "./i18n.svelte";
import type { CloseBehavior } from "./settings";
import { runBoundedStep } from "./bounded-step";

/** 关闭请求来源：窗口控制（X 按钮）或托盘菜单的「退出」。 */
export type CloseRequestSource = "window" | "tray";

export interface WindowClosePorts {
  behavior: (source: CloseRequestSource) => CloseBehavior;
  blocked: () => boolean;
  blockedReason?: () => string;
  hasActiveTasks: () => boolean;
  choose: () => Promise<CloseBehavior | null>;
  confirm: () => Promise<boolean>;
  remember: (behavior: CloseBehavior) => void;
  minimize: () => Promise<void>;
  prepareExit?: () => Promise<boolean | (() => void)>;
  stopPi: () => Promise<void>;
  stopDsh: () => Promise<void>;
  destroy: () => Promise<void>;
  error: (error: unknown) => void;
}

/**
 * 退出链路上每个宿主持有锁的步骤的时间上限。
 *
 * 这些步骤（设置落盘、文件编辑锁、停止 Pi/DSH、销毁窗口）都可能在某把无超时的
 * 锁上永久等待。一旦无限等待，`closingWindow` 就永远为真，而它绑定了整个 shell
 * 的 `inert`——界面渲染正常却吞掉所有输入，且 `pending` 会让之后每一次关闭请求
 * （包括托盘退出）都静默返回，只能强杀进程。加界是为了让这种情况可恢复：
 * 超时后释放界面，并允许再次尝试关闭。
 */
const EXIT_STEP_TIMEOUT_MS = 15_000;

export function createWindowCloseHandler(ports: WindowClosePorts) {
  let pending = false;
  return async (event: { preventDefault(): void }, source: CloseRequestSource = "window") => {
    // Every OS close request must be intercepted, including repeated clicks.
    event.preventDefault();
    if (pending) return;
    pending = true;
    let releaseExit: (() => void) | undefined;
    try {
      if (ports.blocked()) throw new Error(ports.blockedReason?.() ?? t("任务正在切换模式，请完成后再关闭窗口"));
      let behavior = ports.behavior(source);
      if (behavior === "ask") {
        // 等用户选择，不设上限：卡在这里的是人，不是锁。
        const choice = await ports.choose();
        if (!choice || choice === "ask") return;
        behavior = choice;
        ports.remember(choice);
      }
      if (behavior === "minimize") {
        await ports.minimize();
        return;
      }
      if (ports.hasActiveTasks() && !await ports.confirm()) return;

      // 退出流程一旦开始，界面就进入 inert。从这里到流程结束，任何一步无限等待都
      // 会把应用永久钉死，因此每一步都必须有界，且任何出口都要释放界面。
      if (ports.prepareExit) {
        const prepared = await runBoundedStep("close_prepare_exit", ports.prepareExit, EXIT_STEP_TIMEOUT_MS);
        if (!prepared.ok) {
          throw prepared.error;
        }
        if (!prepared.value) {
          return;
        }
        if (typeof prepared.value === "function") releaseExit = prepared.value;
      }
      if (ports.blocked()) throw new Error(ports.blockedReason?.() ?? t("任务正在切换模式，请完成后再关闭窗口"));
      for (const [name, step] of [
        ["close_stop_pi", ports.stopPi],
        ["close_stop_dsh", ports.stopDsh],
        ["close_destroy", ports.destroy],
      ] as const) {
        const result = await runBoundedStep(name, step, EXIT_STEP_TIMEOUT_MS);
        if (!result.ok) throw result.error;
      }
    } catch (error) {
      // 关键：超时也必须让用户回到可用状态，否则「界面还在、点什么都没反应」
      // 就是终局。恢复可用性优先于继续退出。
      ports.error(error);
    } finally {
      // 无论成功、失败还是超时，都要解除 inert 并允许后续重试，
      // 这样托盘退出/再次关闭不会因为一个卡住的步骤而永久失效。
      try { releaseExit?.(); }
      finally {
        pending = false;
      }
    }
  };
}
