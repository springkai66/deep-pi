export interface RuntimeComponent {
  id: "deeppi" | "pi" | "dsh" | "dshmarket";
  name: string;
  currentVersion: string | null;
  source: "system" | "managed" | "development" | "profile";
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
}