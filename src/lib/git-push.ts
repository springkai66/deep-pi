export interface PushTarget { remote: string; destination: string }
export interface PushTargets { sourceRef: string; sourceOid: string; targets: PushTarget[] }
export interface PushPreview extends PushTarget { sourceRef: string; sourceOid: string; targetRef: string }
export interface PushResult { outcome: "pushed" | "upToDate" | "rejected" | "unknown"; sourceOid: string; targetRef: string; detail: string }
export interface RemoteVerification { outcome: "matches" | "different" | "missing"; remoteOid: string | null; sourceOid: string; targetRef: string }
export interface SyncResult { outcome: "synced" | "unknown"; oid: string; references: string[] }
export interface PushPorts {
  load: (projectId: string, operationId: string) => Promise<PushTargets>;
  push: (projectId: string, expected: PushPreview, operationId: string) => Promise<PushResult | null>;
  cancel: (operationId: string) => Promise<void>;
  verify: (projectId: string, expected: PushPreview, operationId: string) => Promise<RemoteVerification>;
  sync: (projectId: string, expected: PushPreview, operationId: string) => Promise<SyncResult | null>;
}
export interface PushState {
  projectId: string | null; targets: PushTargets | null; selected: number; branch: string;
  busy: boolean; phase: "idle" | "loading" | "pushing" | "verifying" | "syncing"; cancelling: boolean;
  error: string; result: PushResult | null;
  attempted: PushPreview | null; verification: RemoteVerification | null;
  syncResult: SyncResult | null;
}

export function createPushController(
  ports: PushPorts, changed: (state: PushState) => void, applied: (projectId: string) => void,
) {
  let state: PushState = { projectId: null, targets: null, selected: -1, branch: "", busy: false, phase: "idle", cancelling: false, error: "", result: null, attempted: null, verification: null, syncResult: null };
  let generation = 0;
  let projectGeneration = 0;
  let active: { id: string; cancelling: boolean } | null = null;
  let disposed = false;
  const emit = (patch: Partial<PushState>) => {
    state = { ...state, ...patch };
    if (!disposed) changed(state);
  };
  return {
    setProject(projectId: string | null) {
      if (disposed || state.projectId === projectId) return;
      generation++;
      projectGeneration++;
      emit({ projectId, targets: null, selected: -1, branch: "", result: null, error: "", attempted: null, verification: null, syncResult: null });
    },
    selectTarget(selected: number) {
      if (!active && !disposed) emit({ selected: Number.isInteger(selected) && state.targets?.targets[selected] ? selected : -1 });
    },
    setBranch(branch: string) { if (!active && !disposed) emit({ branch }); },
    invalidate() { generation++; emit({ targets: null, selected: -1 }); },
    async load() {
      if (active || disposed || !state.projectId) return;
      const projectId = state.projectId;
      const expectedGeneration = ++generation;
      const id = crypto.randomUUID();
      active = { id, cancelling: false };
      emit({ busy: true, phase: "loading", cancelling: false, error: "", targets: null, selected: -1 });
      try {
        const targets = await ports.load(projectId, id);
        if (!disposed && generation === expectedGeneration && !active.cancelling) {
          emit({ targets, branch: targets.sourceRef.startsWith("refs/heads/") ? targets.sourceRef.slice(11) : "" });
        }
      } catch (error) {
        if (!disposed && generation === expectedGeneration) emit({ error: String(error) });
      } finally { active = null; emit({ busy: false, phase: "idle", cancelling: false }); }
    },
    async push() {
      const target = state.targets?.targets[state.selected];
      if (active || disposed || !state.projectId || !state.targets || !target || !state.branch.trim()) return;
      const projectId = state.projectId;
      const expected: PushPreview = { ...target, sourceRef: state.targets.sourceRef, sourceOid: state.targets.sourceOid, targetRef: `refs/heads/${state.branch}` };
      const expectedProjectGeneration = projectGeneration;
      const id = crypto.randomUUID();
      active = { id, cancelling: false };
      const previous = { attempted: state.attempted, result: state.result, verification: state.verification, syncResult: state.syncResult };
      emit({ busy: true, phase: "pushing", cancelling: false, error: "", result: null, attempted: expected, verification: null, syncResult: null });
      let succeeded = false;
      try {
        const result = await ports.push(projectId, expected, id);
        succeeded = result?.outcome === "pushed" || result?.outcome === "upToDate";
        if (!disposed && projectGeneration === expectedProjectGeneration && result) emit({ result, targets: null, selected: -1 });
        if (!disposed && projectGeneration === expectedProjectGeneration && result === null) emit(previous);
      } catch (error) {
        if (!disposed && projectGeneration === expectedProjectGeneration) emit({ targets: null, selected: -1, error: `推送结果未确认，请核对远程目标；不会自动重试：${String(error)}` });
      } finally { active = null; emit({ busy: false, phase: "idle", cancelling: false }); }
      if (succeeded && !disposed) applied(projectId);
    },
    async verify() {
      if (active || disposed || !state.projectId || !state.attempted) return;
      const { projectId, attempted } = state;
      const expectedProjectGeneration = projectGeneration;
      const id = crypto.randomUUID();
      active = { id, cancelling: false };
      emit({ busy: true, phase: "verifying", cancelling: false, error: "", verification: null });
      try {
        const verification = await ports.verify(projectId, attempted, id);
        if (!disposed && projectGeneration === expectedProjectGeneration) {
          emit(active.cancelling ? { error: "已取消远程核对，目标状态尚未确认" } : { verification });
        }
      } catch (error) {
        if (!disposed && projectGeneration === expectedProjectGeneration) {
          emit({ error: active.cancelling ? "已取消远程核对，目标状态尚未确认" : `远程核对未完成，不能据此判断目标状态：${String(error)}` });
        }
      } finally { active = null; emit({ busy: false, phase: "idle", cancelling: false }); }
    },
    async sync() {
      if (active || disposed || !state.projectId || !state.attempted) return;
      const { projectId, attempted } = state;
      const expectedProjectGeneration = projectGeneration;
      const id = crypto.randomUUID();
      active = { id, cancelling: false };
      emit({ busy: true, phase: "syncing", cancelling: false, error: "", syncResult: null });
      let refresh = false;
      try {
        const syncResult = await ports.sync(projectId, attempted, id);
        refresh = syncResult !== null;
        if (!disposed && projectGeneration === expectedProjectGeneration && syncResult) emit({ syncResult });
      } catch (error) {
        refresh = true;
        if (!disposed && projectGeneration === expectedProjectGeneration) {
          emit({ error: `跟踪引用同步未确认完成，请刷新并核对本地状态；不会自动重试：${String(error)}` });
        }
      } finally { active = null; emit({ busy: false, phase: "idle", cancelling: false }); }
      if (refresh && !disposed) applied(projectId);
    },
    async cancel() {
      if (!active || active.cancelling || disposed) return;
      active.cancelling = true;
      const id = active.id;
      emit({ cancelling: true });
      try { await ports.cancel(id); }
      catch (error) { if (active?.id === id) emit({ error: `取消请求发送失败，仍在等待结果：${String(error)}` }); }
    },
    dispose() { disposed = true; generation++; },
  };
}
