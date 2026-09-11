export interface RuntimeComponent {
  id: "deeppi" | "node" | "pi" | "dsh" | "dshmarket";
  name: string;
  currentVersion: string | null;
  source: "managed" | "development" | "profile";
  available: boolean;
}

export interface RuntimeUpdate {
  id: string;
  name: string;
  currentVersion: string | null;
  latestVersion: string | null;
  updateAvailable: boolean;
  installable: boolean;
  canRollback: boolean;
  stale: boolean;
  error: string | null;
  /** 版本说明，例如上游有更新但尚未通过兼容验证。 */
  note: string | null;
}