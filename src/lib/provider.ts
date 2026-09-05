export interface ModelCostConfig {
  input: number | null;
  output: number | null;
  cacheRead: number | null;
  cacheWrite: number | null;
}

export interface ConfiguredModel {
  id: string;
  name: string;
  reasoning: boolean;
  input: string[];
  contextWindow: number;
  maxTokens: number;
  thinkingLevels: string[];
  cost: ModelCostConfig | null;
}

export interface ProviderRecord {
  id: string;
  name: string;
  api: string;
  baseUrl: string | null;
  headers: Record<string, string>;
  proxy: string | null;
  models: ConfiguredModel[];
}

export interface PiModelSummary {
  provider: string;
  id: string;
  name: string;
  path: string;
  contextWindow: number | null;
  inputCost: number | null;
  outputCost: number | null;
  cacheReadCost: number | null;
  cacheWriteCost: number | null;
}

export interface ProviderModelSummary {
  providerId: string;
  id: string;
  name: string;
  contextWindow: number | null;
  maxTokens: number | null;
  reasoning: boolean | null;
  input: string[];
}

export interface PiModelProfile {
  provider: string;
  id: string;
  name: string;
  api: string | null;
  baseUrl: string | null;
  reasoning: boolean;
  input: string[];
  contextWindow: number | null;
  maxTokens: number | null;
  thinkingLevels: string[];
  cost: ModelCostConfig | null;
}

export interface ProviderCredentialStatus {
  providerId: string;
  configured: boolean;
  apiKeyConfigured: boolean;
  nativeAuthConfigured: boolean;
  authType: string | null;
}

export interface ProviderConnectionResult {
  providerId: string;
  reachable: boolean;
  status: number;
}
