import { t } from "./i18n.svelte";
import type { CloseBehavior } from "./settings";

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
      if (ports.prepareExit) {
        const prepared = await ports.prepareExit();
        if (!prepared) return;
        if (typeof prepared === "function") releaseExit = prepared;
      }
      if (ports.blocked()) throw new Error(ports.blockedReason?.() ?? t("任务正在切换模式，请完成后再关闭窗口"));
      await ports.stopPi();
      await ports.stopDsh();
      await ports.destroy();
    } catch (error) {
      ports.error(error);
    } finally {
      try { releaseExit?.(); }
      finally { pending = false; }
    }
  };
}
