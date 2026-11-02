/** Display projections only. No browser value constitutes execution authority. */
export const SECTIONS = [
  "projects",
  "execution",
  "costs-approvals",
  "connections",
  "results-activity",
] as const;
export type Section = (typeof SECTIONS)[number];
export type TaskStatus =
  | "planned"
  | "ready"
  | "blocked"
  | "running"
  | "awaiting_verification"
  | "verified"
  | "failed"
  | "paused"
  | "unknown";
export type Role = "owner" | "editor" | "viewer";
export type Availability =
  | "live"
  | "stale"
  | "offline"
  | "connecting"
  | "unauthorized"
  | "forbidden"
  | "unavailable";
export interface Money {
  currency: string;
  minor: string;
}
export interface Task {
  id: string;
  title: string;
  kind: "group" | "leaf";
  parent: string | null;
  dependencies: string[];
  status: TaskStatus;
  model: string | null;
  effort: string | null;
  mode: "api" | "cli" | null;
  scope: string[];
  criteria: string[];
  blockers: string[];
}
export interface Attempt {
  id: string;
  task: string;
  epoch: number;
  worker: string | null;
  provider: string | null;
  connection: string | null;
  state:
    | "running"
    | "terminated"
    | "unknown"
    | "fenced"
    | "admitted"
    | "accepted"
    | "blocked"
    | "awaiting_verification";
  admitted?: string;
  leaseUntil?: number;
  observedAt?: number;
  logsAvailable?: boolean;
  started: string | null;
  ended: string | null;
  phase: string;
  steps: { label: string; at: string; complete: boolean }[];
  logs: string[];
}
export interface Connection {
  id: string;
  provider: "github" | "openai" | "anthropic" | "aws" | "other";
  label: string;
  account: string;
  mode: "api" | "cli" | "repository" | "infrastructure";
  status:
    | "fixture_verified"
    | "unverified"
    | "revoked"
    | "unsupported"
    | "registered"
    | "recorded_verified"
    | "expired"
    | "blocked"
    | "unavailable";
  operational?: boolean;
  models?: string[];
  verifiedUntil?: number | null;
  scope: string;
  version: number;
  reason: string;
}
export interface Result {
  id: string;
  task: string;
  attempt: string;
  accepted: boolean;
  verification: "passed" | "pending" | "failed";
  tests: string[];
  sha256: string;
  ref: string | null;
  published: boolean;
  at: string;
  fingerprint?: string;
  epoch?: number;
  publicationState?:
    | "not_prepared"
    | "prepared"
    | "submitting"
    | "published"
    | "unknown"
    | "blocked";
  acceptance?: string | null;
}
export interface Activity {
  source?: string;
  sequence?: number;
  id: string;
  task: string | null;
  at: string | null;
  kind: "execution" | "verification" | "financial" | "security" | "publication";
  title: string;
  detail: string;
}
export interface Finance {
  currency: string;
  version: number;
  ceiling: string;
  reserved: string;
  settled: string;
  provisional: string;
  available: string | null;
  unknownLiabilities: number;
  paused: boolean;
  overCeiling?: boolean;
  forecasts: {
    optimistic: string;
    expected: string;
    pessimistic: string;
  } | null;
  categories: {
    id: "ai" | "compute" | "storage" | "transfer";
    settled: string | null;
    reserved: string;
    forecast: string | null;
    provisional?: string;
  }[];
  approvals: {
    id: string;
    actor: string;
    previous: string;
    total: string;
    version: number;
    at: string;
  }[];
}
export interface Snapshot {
  schema: 1 | 2;
  eventCursor?: string;
  pages?: Partial<Record<PageResource, string | null>>;
  provenance: "controller" | "demo";
  revision: number;
  cursor: number;
  asOf: string;
  role: Role;
  project: {
    id: string;
    name: string;
    goal: string;
    repository: string | null;
    repositoryId: string | null;
  };
  plan: { id: string; version: number; fingerprint: string; complete: boolean };
  tasks: Task[];
  attempts: Attempt[];
  finance: Finance | null;
  connections: Connection[];
  results: Result[];
  activity: Activity[];
  capabilities: {
    execution?: boolean;
    activity?: boolean;
    budgetIncrease?: boolean;
    events: boolean;
    finance: boolean;
    connections: boolean;
    results: boolean;
    mutations: false;
  };
}
export interface ProjectSummary {
  id: string;
  name: string;
  detail: string;
}
export type PageResource =
  | "attempts"
  | "results"
  | "approvals"
  | `connections/${"model" | "github" | "aws"}`
  | `activity/${"scheduler" | "worker" | "activation" | "finance" | "governance" | "provider" | "publication"}`;
export interface OperationalPage {
  schema: 2;
  project: string;
  resource: PageResource;
  role: Role;
  items:
    Attempt[] | Connection[] | Result[] | Activity[] | Finance["approvals"];
  next: string | null;
}
export interface ReadSource {
  projects(signal: AbortSignal): Promise<ProjectSummary[]>;
  snapshot(project: string, signal: AbortSignal): Promise<Snapshot>;
  page?(
    project: string,
    resource: PageResource,
    cursor: string,
    snapshot: Snapshot,
    signal: AbortSignal,
  ): Promise<OperationalPage>;
}
export const isId = (id: string): boolean =>
  /^[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}$/.test(id);
export const statusLabels: Record<TaskStatus, string> = {
  planned: "Planned",
  ready: "Ready",
  blocked: "Blocked",
  running: "Running",
  awaiting_verification: "Awaiting verification",
  verified: "Verified",
  failed: "Failed",
  paused: "Paused",
  unknown: "Unknown",
};
