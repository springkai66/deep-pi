import type { DialogRequest, DialogValue } from "./dialog";
import { t } from "./i18n.svelte";
import type { PiAuthProviderInfo } from "./pi-auth";
import { supportsProviderLogin } from "./slash-commands";
import type { Task } from "./task";

export type SensitiveHostSlash = "import" | "share" | "trust" | "login" | "logout" | "reload" | "quit";

type HostInvoke = typeof import("@tauri-apps/api/core").invoke;

export interface SensitiveHostSlashPorts {
  invoke: HostInvoke;
  confirm: (title: string, message: string, confirmLabel?: string) => Promise<boolean>;
  dialog: (request: Omit<DialogRequest, "id" | "resolve">) => Promise<DialogValue>;
  chooseImport: () => Promise<string | string[] | null>;
  reportError: (error: unknown) => void;
  openLogin: (provider: string | null) => void;
  notifyModelsChanged: () => void;
  restart: (task: Task) => Promise<void>;
  quit: (() => Promise<boolean>) | null;
}

export async function runSensitiveHostSlash(task: Task, command: SensitiveHostSlash, args: string, ports: SensitiveHostSlashPorts): Promise<boolean> {
  const { invoke, confirm, dialog, reportError } = ports;
  switch (command) {
    case "import": {
      if (!task.runId) { reportError(t("请先启动会话后再使用命令")); return false; }
      const picked = args || await ports.chooseImport();
      const source = Array.isArray(picked) ? picked[0] : picked;
      if (!source) return false;
      const request = { taskId: task.id, runId: task.runId, source };
      const preview = await invoke<{ sessionId: string }>("pi_slash_import", { ...request, confirmed: false });
      if (!await confirm(t("导入 Pi 会话"), t("将当前任务切换到会话 {id}？原会话文件会保留。", { id: preview.sessionId }))) return false;
      const outcome = await invoke<{ cancelled: boolean }>("pi_slash_import", { ...request, confirmed: true, expectedSessionId: preview.sessionId });
      return !outcome.cancelled;
    }
    case "share": {
      if (!task.runId) { reportError(t("请先启动会话后再使用命令")); return false; }
      if (!await confirm(t("分享 Pi 会话"), t("将当前会话发布为私密 GitHub Gist？持有链接的人可以查看内容。"), t("发布"))) return false;
      const url = await invoke<string>("pi_slash_share", { taskId: task.id, runId: task.runId });
      await dialog({ kind: "alert", title: t("分享完成"), message: url });
      return true;
    }
    case "trust": {
      const previous = await invoke<{ path: string; decision: boolean } | null>("pi_slash_config", { taskId: task.id, action: "get_trust", payload: null });
      const decision = await dialog({ kind: "choice", title: t("项目信任"),
        message: `${task.projectPath}\n${previous ? `${previous.path}: ${previous.decision ? t("已信任") : t("不信任")}` : t("尚无已保存的决定")}`,
        choices: [{ value: "trust", label: t("信任此项目") }, { value: "deny", label: t("不信任此项目") }],
      });
      if (decision !== "trust" && decision !== "deny") return false;
      if (!await confirm(t("保存项目信任决定"), decision === "trust"
        ? t("信任此项目的本地资源？项目扩展可能执行代码。重新加载会话后生效。")
        : t("不信任此项目的本地资源？重新加载会话后生效。"))) return false;
      await invoke("pi_slash_config", { taskId: task.id, action: "set_trust", payload: decision === "trust" });
      await dialog({ kind: "alert", title: t("项目信任"), message: t("决定已保存。重新加载会话后生效。") });
      return true;
    }
    case "login": {
      if (args) {
        const providers = await invoke<PiAuthProviderInfo[]>("pi_auth_providers");
        if (!supportsProviderLogin(args, providers)) {
          reportError(t("不支持登录的 Provider：{name}", { name: args }));
          return false;
        }
      }
      ports.openLogin(args || null);
      return true;
    }
    case "logout": {
      const status = await invoke<{ credentials: { provider: string }[] }>("pi_auth_status");
      const providers = status.credentials.map((entry) => entry.provider);
      if (!providers.length) { reportError(t("没有已登录的 Provider")); return false; }
      const selected = args || await dialog({
        kind: "choice", title: t("退出 Provider 登录"), message: t("选择要移除凭据的 Provider"),
        choices: providers.map((provider) => ({ value: provider, label: provider })),
      });
      if (typeof selected !== "string" || !providers.includes(selected)) return false;
      if (!await confirm(t("退出 Provider 登录"), t("移除 {name} 的登录凭据？", { name: selected }))) return false;
      await invoke("pi_auth_logout", { request: { providerId: selected } });
      ports.notifyModelsChanged();
      return true;
    }
    case "reload": {
      const before = task.runId;
      if (before) {
        const state = await invoke<{ isStreaming: boolean; isCompacting: boolean }>("rpc_command", {
          taskId: task.id, runId: before, command: { type: "get_state" },
        });
        if (state.isStreaming || state.isCompacting) { reportError(t("请等待当前回合完成后再重新加载")); return false; }
        await invoke("stop_rpc_task", { taskId: task.id, runId: before });
      }
      await ports.restart(task);
      return !!task.runId && task.runId !== before;
    }
    case "quit": {
      if (!ports.quit || !await confirm(t("退出 DeepPi"), t("确定要退出 DeepPi 吗？"))) return false;
      return ports.quit();
    }
  }
}
