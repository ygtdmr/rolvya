import type { ReadSource, Snapshot } from "../api/model";
export const DEMO_PROJECT = "prj_atlas";
const at = (minute: string) => `2026-11-01T09:${minute}:00Z`;
export function demoSnapshot(project = DEMO_PROJECT): Snapshot {
  const second = project === "prj_relay";
  return {
    schema: 1,
    provenance: "demo",
    revision: 1,
    cursor: 1,
    asOf: at("42"),
    role: "owner",
    project: {
      id: project,
      name: second ? "Relay / event gateway" : "Atlas / developer platform",
      goal: second
        ? "Make event delivery explicit, attributable and resilient."
        : "Ship a dependable developer API with independently verified changes and bounded execution.",
      repository: second ? "example/relay" : "example/atlas",
      repositoryId: second ? "900002" : "900001",
    },
    plan: {
      id: "plan_v3",
      version: 3,
      fingerprint:
        "74bce842dffb1a900d98a47134bc68b2a842cc21125752193dfd733edbced8e1",
      complete: true,
    },
    tasks: [
      {
        id: "G",
        title: "Deliver the API foundation",
        kind: "group",
        parent: null,
        dependencies: [],
        status: "awaiting_verification",
        model: null,
        effort: null,
        mode: null,
        scope: ["src/", "tests/"],
        criteria: [
          "A and B independently accepted",
          "Group acceptance verified",
        ],
        blockers: [
          "Waiting for B acceptance. A group never receives a worker.",
        ],
      },
      {
        id: "A",
        title: "Harden request validation",
        kind: "leaf",
        parent: "G",
        dependencies: [],
        status: "verified",
        model: "OpenAI fixture model",
        effort: "high",
        mode: "api",
        scope: ["src/alpha.rs", "tests/alpha_test.rs"],
        criteria: [
          "Reject malformed requests",
          "Independent validation tests pass",
        ],
        blockers: [],
      },
      {
        id: "B",
        title: "Build the response pipeline",
        kind: "leaf",
        parent: "G",
        dependencies: [],
        status: "running",
        model: "Anthropic fixture model",
        effort: "bounded thinking",
        mode: "api",
        scope: ["src/beta.rs", "tests/beta_test.rs"],
        criteria: [
          "Preserve typed error responses",
          "Independent response tests pass",
        ],
        blockers: [],
      },
      {
        id: "C",
        title: "Verify the integrated contract",
        kind: "leaf",
        parent: null,
        dependencies: ["G"],
        status: "blocked",
        model: "OpenAI fixture model",
        effort: "medium",
        mode: "api",
        scope: ["tests/integration_test.rs"],
        criteria: ["Consume accepted A/B evidence", "Contract tests pass"],
        blockers: [
          "Requires accepted group G. Process completion alone does not satisfy this dependency.",
        ],
      },
    ],
    attempts: [
      {
        id: "attempt_A1",
        task: "A",
        epoch: 1,
        worker: "worker-7c2a",
        provider: "AWS Fargate · synthetic",
        connection: "openai_A",
        state: "terminated",
        started: at("12"),
        ended: at("29"),
        phase: "Independently accepted",
        steps: [
          {
            label: "Admitted · exposure reserved",
            at: at("12"),
            complete: true,
          },
          { label: "Authenticated worker", at: at("13"), complete: true },
          { label: "Candidate submitted", at: at("24"), complete: true },
          { label: "Independent verification", at: at("28"), complete: true },
          { label: "R09 acceptance", at: at("29"), complete: true },
        ],
        logs: [
          "[redacted fixture] Source commitment verified.",
          "[redacted fixture] Candidate accepted; provider credentials never delivered.",
        ],
      },
      {
        id: "attempt_B1",
        task: "B",
        epoch: 1,
        worker: "worker-9f31",
        provider: "AWS Fargate · synthetic",
        connection: "anthropic_B",
        state: "running",
        started: at("14"),
        ended: null,
        phase: "Applying bounded changes",
        steps: [
          {
            label: "Admitted · exposure reserved",
            at: at("14"),
            complete: true,
          },
          { label: "Authenticated worker", at: at("15"), complete: true },
          { label: "Candidate submitted", at: at("43"), complete: false },
          { label: "Independent verification", at: at("44"), complete: false },
          { label: "R09 acceptance", at: at("45"), complete: false },
        ],
        logs: [
          "[redacted fixture] Pinned source received.",
          "[redacted fixture] Editing src/beta.rs within approved scope.",
        ],
      },
    ],
    finance: {
      currency: "USD",
      version: 4,
      ceiling: "24000",
      reserved: "12000",
      settled: "2240",
      provisional: "1800",
      available: null,
      unknownLiabilities: 1,
      paused: true,
      forecasts: {
        optimistic: "9600",
        expected: "12400",
        pessimistic: "17000",
      },
      categories: [
        { id: "ai", settled: "1640", reserved: "8200", forecast: "8800" },
        { id: "compute", settled: "500", reserved: "3000", forecast: "2900" },
        { id: "storage", settled: "100", reserved: "400", forecast: "400" },
        { id: "transfer", settled: null, reserved: "400", forecast: "300" },
      ],
      approvals: [
        {
          id: "approval_004",
          actor: "Synthetic Owner",
          previous: "18000",
          total: "24000",
          version: 4,
          at: at("05"),
        },
      ],
    },
    connections: [
      {
        id: "github_1",
        provider: "github",
        label: "GitHub App",
        account: "example / atlas",
        mode: "repository",
        status: "fixture_verified",
        scope: project,
        version: 2,
        reason:
          "Synthetic installation. Live App permission and repository binding remain unavailable.",
      },
      {
        id: "openai_A",
        provider: "openai",
        label: "OpenAI API",
        account: "API account A",
        mode: "api",
        status: "fixture_verified",
        scope: project,
        version: 3,
        reason: "Private protocol fixture; no real account or API charges.",
      },
      {
        id: "anthropic_B",
        provider: "anthropic",
        label: "Anthropic API",
        account: "API account B",
        mode: "api",
        status: "fixture_verified",
        scope: project,
        version: 2,
        reason:
          "Private protocol fixture; separately scoped account and credential reference.",
      },
      {
        id: "codex_C",
        provider: "openai",
        label: "Codex CLI",
        account: "CLI account C",
        mode: "cli",
        status: "unverified",
        scope: project,
        version: 1,
        reason:
          "Hosted integration permission, account entitlement and supported authentication are unverified.",
      },
      {
        id: "claude_D",
        provider: "anthropic",
        label: "Claude Code CLI",
        account: "CLI account D",
        mode: "cli",
        status: "unsupported",
        scope: project,
        version: 1,
        reason:
          "Subscription login does not establish permission for third-party hosted execution. No API fallback.",
      },
      {
        id: "aws_1",
        provider: "aws",
        label: "AWS Fargate",
        account: "Infrastructure account E",
        mode: "infrastructure",
        status: "unverified",
        scope: project,
        version: 1,
        reason:
          "Real IAM confinement, network isolation and enforceable exposure require independent validation.",
      },
    ],
    results: [
      {
        id: "result_A1",
        task: "A",
        attempt: "attempt_A1",
        accepted: true,
        verification: "passed",
        tests: ["rejects_invalid_input", "preserves_request_binding"],
        sha256: "a1".repeat(32),
        ref: "refs/heads/rolvya/prj_atlas/A/attempt_A1",
        published: true,
        at: at("30"),
      },
      {
        id: "candidate_B1",
        task: "B",
        attempt: "attempt_B1",
        accepted: false,
        verification: "pending",
        tests: [],
        sha256: "b2".repeat(32),
        ref: null,
        published: false,
        at: at("41"),
      },
    ],
    activity: [
      {
        id: "event_6",
        task: "B",
        at: at("41"),
        kind: "execution",
        title: "Response pipeline is running",
        detail:
          "Synthetic worker is applying changes. No independent success has been claimed.",
      },
      {
        id: "event_5",
        task: null,
        at: at("35"),
        kind: "financial",
        title: "New billable admission paused",
        detail:
          "One external liability is unresolved. Existing holds remain intact.",
      },
      {
        id: "event_4",
        task: "A",
        at: at("30"),
        kind: "publication",
        title: "Isolated reference reconciled",
        detail:
          "Simulated remote fixture only. No protected branch write or merge.",
      },
      {
        id: "event_3",
        task: "A",
        at: at("29"),
        kind: "verification",
        title: "Request validation independently accepted",
        detail:
          "Synthetic verifier evidence passed exact source and change-set checks.",
      },
      {
        id: "event_2",
        task: "B",
        at: at("14"),
        kind: "execution",
        title: "Parallel task admitted",
        detail: "Separate account, attempt and financial reservation.",
      },
      {
        id: "event_1",
        task: null,
        at: at("05"),
        kind: "financial",
        title: "Synthetic approval recorded",
        detail: "Fixture ceiling version 4. No live budget was modified.",
      },
    ],
    capabilities: {
      events: false,
      finance: true,
      connections: true,
      results: true,
      mutations: false,
    },
  };
}
export function advanceDemo(snapshot: Snapshot): Snapshot {
  const next = structuredClone(snapshot),
    b = next.tasks.find((t) => t.id === "B"),
    g = next.tasks.find((t) => t.id === "G"),
    c = next.tasks.find((t) => t.id === "C");
  if (!b || !g || !c) return next;
  if (b.status === "running") {
    b.status = "awaiting_verification";
    const attempt = next.attempts.find((a) => a.task === "B");
    if (attempt) {
      attempt.state = "terminated";
      attempt.phase = "Candidate awaits independent verification";
      attempt.ended = at("43");
      attempt.steps[2]!.complete = true;
    }
  } else if (b.status === "awaiting_verification") {
    b.status = "verified";
    g.status = "verified";
    g.blockers = [];
    c.status = "ready";
    c.blockers = [];
    const result = next.results.find((r) => r.task === "B");
    if (result) {
      result.accepted = true;
      result.verification = "passed";
      result.tests = ["typed_response_contract"];
    }
    const attempt = next.attempts.find((a) => a.task === "B");
    if (attempt) {
      attempt.phase = "Independently accepted";
      attempt.steps.forEach((s) => (s.complete = true));
    }
  }
  next.revision++;
  next.cursor++;
  return next;
}
export const demoSource: ReadSource = {
  async projects(signal) {
    signal.throwIfAborted();
    return [
      {
        id: DEMO_PROJECT,
        name: "Atlas / developer platform",
        detail: "API foundation · plan v3",
      },
      {
        id: "prj_relay",
        name: "Relay / event gateway",
        detail: "Independent synthetic workspace",
      },
    ];
  },
  async snapshot(project, signal) {
    signal.throwIfAborted();
    const snapshot = demoSnapshot(project);
    const n = Number(new URLSearchParams(location.search).get("nodes"));
    if ([200, 1000, 5000].includes(n))
      snapshot.tasks = Array.from({ length: n }, (_, i) => ({
        id: `T_${i}`,
        title: `Benchmark task ${i}`,
        kind: "leaf",
        parent: null,
        dependencies: i >= 100 ? [`T_${i - 100}`] : [],
        status: i % 3 === 0 ? "verified" : "planned",
        model: "Synthetic benchmark model",
        effort: "medium",
        mode: "api",
        scope: ["src/"],
        criteria: ["Deterministic fixture"],
        blockers: [],
      }));
    return snapshot;
  },
};
