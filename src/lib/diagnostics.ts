export const DIAGNOSTIC_LABELS = {
  process_starting: "进程启动", spawn_failed: "进程创建失败", process_exit: "进程退出",
  stop_requested: "请求停止", stop_timeout: "停止超时", input_write_failed: "输入管道写入失败",
  output_invalid: "输出协议或读取失败", stderr_observed: "标准错误输出（字节）",
  stderr_read_failed: "标准错误管道读取失败", request_timeout: "请求超时",
  response_failed: "命令失败", input_queue_full: "输入队列已满或关闭", replay_gap: "事件回放缺口",
  spawn_not_found: "找不到进程程序或工作目录", spawn_denied: "进程启动被拒绝",
  process_ownership_failed: "进程树托管失败", process_resume_failed: "挂起进程恢复失败",
  frame_too_large: "输出帧超过大小上限", invalid_json: "输出不是合法的 JSON 对象",
  incomplete_frame: "进程退出时输出帧不完整", output_read_failed: "输出管道读取失败",
  history_failed: "历史响应解析或临时存储失败",
} as const;

const SAFE_DIAGNOSTIC_ERRORS = new Set([
  "诊断记录暂不可用", "诊断报告正在导出", "诊断预览已失效，请刷新后重新导出",
  "无法生成诊断报告", "主窗口已关闭", "不支持该报告保存位置", "诊断导出任务失败",
  "报告需要完整的普通文件路径", "报告不支持设备路径", "报告文件名无效",
  "无法创建报告，请选择可写目录和新的文件名；不会覆盖已有文件",
  "报告写入未完成，目标可能保留不完整文件，请核对后另选文件名",
]);

export interface DiagnosticEvent {
  sequence: number;
  elapsedMs: number;
  run: number;
  code: keyof typeof DIAGNOSTIC_LABELS;
  count: number;
  exitCode: number | null;
}
export interface DiagnosticReport {
  schemaVersion: number;
  snapshotId: string;
  appVersion: string;
  os: string;
  arch: string;
  elapsedMs: number;
  droppedEvents: number;
  events: DiagnosticEvent[];
}
export interface DiagnosticsState {
  report: DiagnosticReport | null;
  busy: boolean;
  error: string;
  status: string;
}
interface Ports {
  invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>;
  confirm: () => Promise<boolean>;
  publish: (state: DiagnosticsState) => void;
  busy: (busy: boolean) => void;
}

export function createDiagnosticsController(ports: Ports) {
  let state: DiagnosticsState = { report: null, busy: false, error: "", status: "" };
  let disposed = false;
  const publish = (patch: Partial<DiagnosticsState>) => {
    state = { ...state, ...patch };
    if (!disposed) ports.publish(state);
  };
  publish({});
  async function operation(action: () => Promise<void>, failure: string) {
    if (disposed || state.busy) return;
    publish({ busy: true, error: "", status: "" });
    ports.busy(true);
    try { await action(); }
    catch (error) { publish({ error: typeof error === "string" && SAFE_DIAGNOSTIC_ERRORS.has(error) ? error : failure }); }
    finally { publish({ busy: false }); ports.busy(false); }
  }
  async function load() {
    const report = await ports.invoke("diagnostics_snapshot") as DiagnosticReport;
    if (!disposed) publish({ report });
  }
  return {
    refresh: () => operation(load, "无法读取诊断，请重试"),
    export: () => operation(async () => {
      if (!state.report) return;
      const saved = await ports.invoke("diagnostics_export", { snapshotId: state.report.snapshotId });
      publish({ status: saved ? "报告已导出" : "已取消导出" });
    }, "无法确认报告是否导出；请检查保存位置。预览过期时请刷新后重试"),
    clear: () => operation(async () => {
      if (!await ports.confirm() || disposed) return;
      // Once clearing is requested, the old preview must not remain exportable after an IPC failure.
      publish({ report: null });
      await ports.invoke("diagnostics_clear");
      if (disposed) return;
      await load();
      publish({ status: "诊断记录已清空" });
    }, "清空或刷新未完成，请重新读取诊断记录"),
    dispose: () => { disposed = true; },
  };
}
