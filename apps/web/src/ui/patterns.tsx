import type {
  Attempt,
  Connection,
  Finance,
  Result,
  Snapshot,
  Task,
} from "../api/model";
import { useLocale, formatMoney } from "../i18n/locale";
import { Button, Icon, Select } from "./controls";
import { Badge, Progress, StatusBadge, Surface } from "./data";
export function ProjectSelector({
  projects,
  value,
  onChange,
}: {
  projects: { id: string; name: string }[];
  value: string;
  onChange: (id: string) => void;
}) {
  return (
    <Select
      label="Project workspace"
      value={value}
      onChange={(e) => onChange(e.target.value)}
    >
      {projects.map((p) => (
        <option key={p.id} value={p.id}>
          {p.name}
        </option>
      ))}
    </Select>
  );
}
export function ProjectSummary({ snapshot }: { snapshot: Snapshot }) {
  return (
    <div className="rv-project-summary">
      <div className="rv-repo-label">
        <Icon name="github" />
        <span>
          {snapshot.project.repository ?? "Remote repository unavailable"}
        </span>
        <Badge>Remote only</Badge>
      </div>
      <p>{snapshot.project.goal}</p>
      <dl className="rv-project-meta">
        <div>
          <dt>Plan</dt>
          <dd>v{snapshot.plan.version} · frozen</dd>
        </div>
        <div>
          <dt>Repository ID</dt>
          <dd className="rv-mono">
            {snapshot.project.repositoryId ?? "Unavailable"}
          </dd>
        </div>
        <div>
          <dt>Fingerprint · SHA-256</dt>
          <dd>
            <code>{snapshot.plan.fingerprint.slice(0, 16)}…</code>
          </dd>
        </div>
      </dl>
    </div>
  );
}
export function TaskTree({
  tasks,
  selected,
  onSelect,
  query = "",
}: {
  tasks: Task[];
  selected: string | null;
  onSelect: (id: string) => void;
  query?: string;
}) {
  const visible = tasks.filter(
    (t) =>
      !query ||
      `${t.id} ${t.title} ${t.status}`
        .toLowerCase()
        .includes(query.toLowerCase()),
  );
  return (
    <ul className="rv-task-tree" aria-label="Task hierarchy">
      {visible.map((task) => (
        <li key={task.id} data-child={task.parent !== null}>
          <button
            className="rv-task-row"
            data-selected={selected === task.id}
            aria-pressed={selected === task.id}
            onClick={() => onSelect(task.id)}
          >
            <span className="rv-task-letter" data-kind={task.kind}>
              {task.id}
            </span>
            <span className="rv-task-copy">
              <strong>{task.title}</strong>
              <small>
                {task.kind === "group" ? "Non-executable group" : task.model}{" "}
                {task.parent ? `· group ${task.parent}` : ""}
              </small>
            </span>
            <StatusBadge status={task.status} />
            <Icon name="chevron" />
          </button>
        </li>
      ))}
      {visible.length === 0 && (
        <li className="rv-table-empty">No tasks match this search.</li>
      )}
    </ul>
  );
}
export function TaskInspector({
  task,
  snapshot,
  onNavigate,
}: {
  task: Task;
  snapshot: Snapshot;
  onNavigate: (section: "execution" | "results-activity") => void;
}) {
  const { t } = useLocale();
  const attempt = snapshot.attempts.find((a) => a.task === task.id);
  return (
    <div className="rv-section-stack">
      <div>
        <p className="rv-eyebrow">
          {task.kind === "group" ? "Non-executable group" : "Executable leaf"} ·{" "}
          {task.id}
        </p>
        <h3 className="rv-inspector-title">{task.title}</h3>
        <StatusBadge status={task.status} />
      </div>
      {task.kind === "group" && (
        <div className="rv-note">
          Groups aggregate accepted descendants. They never launch a worker.
        </div>
      )}
      <dl className="rv-key-values">
        <dt>Project</dt>
        <dd>{snapshot.project.name}</dd>
        <dt>Plan version</dt>
        <dd>{snapshot.plan.version} · immutable</dd>
        <dt>Model</dt>
        <dd>{task.model ?? "No model assignment"}</dd>
        <dt>Effort</dt>
        <dd>{task.effort ?? "Not applicable"}</dd>
        <dt>Access mode</dt>
        <dd>{task.mode ?? "No execution"}</dd>
        <dt>Dependencies</dt>
        <dd>{task.dependencies.join(", ") || "No upstream dependencies"}</dd>
        <dt>Attempt</dt>
        <dd>{attempt?.id ?? "No attempt admitted"}</dd>
      </dl>
      <div>
        <h3>Approved file scope</h3>
        <ul className="rv-code-list">
          {task.scope.map((p) => (
            <li key={p}>
              <code>{p}</code>
            </li>
          ))}
        </ul>
      </div>
      <div>
        <h3>Acceptance criteria</h3>
        <ul className="rv-list">
          {task.criteria.map((c) => (
            <li key={c}>{c}</li>
          ))}
        </ul>
      </div>
      {task.blockers.length > 0 && (
        <div className="rv-note">
          <h3>Why this is blocked</h3>
          {task.blockers.map((b) => (
            <p key={b}>{b}</p>
          ))}
        </div>
      )}
      <div className="rv-inspector-actions">
        <Button icon="execution" onClick={() => onNavigate("execution")}>
          {t("execution")}
        </Button>
        <Button
          variant="secondary"
          icon="results"
          onClick={() => onNavigate("results-activity")}
        >
          {t("results-activity")}
        </Button>
      </div>
      <p className="rv-help">
        Inspection is read-only. A browser selection cannot change the frozen
        plan, model or authority.
      </p>
    </div>
  );
}
export function AttemptTimeline({ attempt }: { attempt: Attempt }) {
  const { date } = useLocale();
  return (
    <ol className="rv-timeline" aria-label={`Attempt ${attempt.id} timeline`}>
      {attempt.steps.map((s) => (
        <li key={s.label} data-complete={s.complete}>
          <span className="rv-timeline-dot">{s.complete ? "✓" : "·"}</span>
          <div>
            <strong>{s.label}</strong>
            <small>
              {s.complete ? date(s.at) : "Pending trusted evidence"}
            </small>
          </div>
        </li>
      ))}
    </ol>
  );
}
export function WorkerStatus({ attempt }: { attempt: Attempt }) {
  return (
    <div className="rv-worker">
      <span className="rv-worker-symbol">
        <Icon name="execution" />
      </span>
      <div>
        <strong>{attempt.worker}</strong>
        <small>{attempt.provider}</small>
      </div>
      <Badge
        tone={
          attempt.state === "running"
            ? "accent"
            : attempt.state === "unknown"
              ? "caution"
              : "neutral"
        }
      >
        {attempt.state}
      </Badge>
    </div>
  );
}
export function BudgetMeter({ finance }: { finance: Finance }) {
  const known =
      BigInt(finance.reserved) +
      BigInt(finance.settled) +
      BigInt(finance.provisional),
    ceiling = BigInt(finance.ceiling);
  const ratio = ceiling === 0n ? 0n : (known * 100n) / ceiling;
  const exceeded = known > ceiling;
  return (
    <div className="rv-budget-meter">
      <Progress
        label="Known exposure against approved ceiling"
        value={exceeded ? 100 : Number(ratio)}
      />
      {exceeded && (
        <p className="rv-field-error">
          Known exposure exceeds the approved ceiling. New spending requires
          controller review.
        </p>
      )}
      <p className="rv-help">
        {ceiling === 0n && known > 0n
          ? "No positive ceiling"
          : `${ratio}% known exposure`}{" "}
        · {finance.unknownLiabilities} unresolved{" "}
        {finance.unknownLiabilities === 1 ? "liability" : "liabilities"} outside
        this visual proportion
      </p>
    </div>
  );
}
export function CostSummary({ finance }: { finance: Finance }) {
  const { locale } = useLocale();
  return (
    <dl className="rv-cost-summary">
      <div>
        <dt>Approved ceiling</dt>
        <dd>{formatMoney(finance.ceiling, finance.currency, locale)}</dd>
      </div>
      <div>
        <dt>Available headroom</dt>
        <dd>
          {finance.available === null
            ? "Unconfirmed"
            : formatMoney(finance.available, finance.currency, locale)}
        </dd>
      </div>
    </dl>
  );
}
export function CostBreakdown({ finance }: { finance: Finance }) {
  const { locale } = useLocale();
  const labels = {
    ai: "AI API",
    compute: "Cloud compute",
    storage: "Storage",
    transfer: "Transfer",
  };
  return (
    <div className="rv-breakdown">
      {finance.categories.map((c) => (
        <div key={c.id}>
          <span>{labels[c.id]}</span>
          <strong>{formatMoney(c.settled, finance.currency, locale)}</strong>
          <small>
            Reserved {formatMoney(c.reserved, finance.currency, locale)}
          </small>
        </div>
      ))}
    </div>
  );
}
export function ApprovalPreview({
  finance,
  project,
  total,
}: {
  finance: Finance;
  project: string;
  total: string;
}) {
  const { locale } = useLocale();
  return (
    <div className="rv-note" data-ui="approval-preview">
      <p className="rv-eyebrow">Read-only proposal preview</p>
      <h3>Budget ceiling change</h3>
      <dl className="rv-key-values">
        <dt>Exact project</dt>
        <dd>{project}</dd>
        <dt>Current ceiling</dt>
        <dd>{formatMoney(finance.ceiling, finance.currency, locale)}</dd>
        <dt>Proposed ceiling</dt>
        <dd>{formatMoney(total, finance.currency, locale)}</dd>
        <dt>Expected version</dt>
        <dd>{finance.version}</dd>
        <dt>Actor</dt>
        <dd>Requires current verified Owner</dd>
        <dt>Effect</dt>
        <dd>Ceiling only. No worker dispatch or authority expansion.</dd>
      </dl>
    </div>
  );
}
export function CredentialModeIndicator({
  connection,
}: {
  connection: Connection;
}) {
  return (
    <Badge icon="lock">
      {connection.mode === "api"
        ? "API · separately billed"
        : connection.mode === "cli"
          ? "CLI subscription"
          : connection.mode === "repository"
            ? "GitHub App"
            : "Infrastructure"}
    </Badge>
  );
}
export function ConnectionCard({
  connection,
  onInspect,
}: {
  connection: Connection;
  onInspect: () => void;
}) {
  return (
    <Surface className="rv-connection-card">
      <div className="rv-connection-heading">
        <span className="rv-connection-symbol">
          <Icon
            name={
              connection.provider === "github"
                ? "github"
                : connection.provider === "aws"
                  ? "layers"
                  : "connections"
            }
          />
        </span>
        <Badge
          tone={
            connection.status === "fixture_verified" ? "positive" : "caution"
          }
        >
          {connection.status === "fixture_verified"
            ? "Fixture verified"
            : connection.status === "unverified"
              ? "Unverified"
              : "Unavailable"}
        </Badge>
      </div>
      <h2>{connection.label}</h2>
      <p>{connection.account}</p>
      <CredentialModeIndicator connection={connection} />
      <dl className="rv-key-values">
        <dt>Project scope</dt>
        <dd>
          <code>{connection.scope}</code>
        </dd>
        <dt>Version</dt>
        <dd>{connection.version}</dd>
      </dl>
      <p className="rv-help">{connection.reason}</p>
      <Button variant="secondary" icon="shield" onClick={onInspect}>
        Inspect authority
      </Button>
    </Surface>
  );
}
export function ResultEvidence({ result }: { result: Result }) {
  return (
    <div className="rv-result-evidence">
      <Badge
        tone={
          result.accepted
            ? "positive"
            : result.verification === "failed"
              ? "danger"
              : "caution"
        }
        icon={result.accepted ? "shield" : "clock"}
      >
        {result.accepted
          ? "Independently accepted"
          : result.verification === "failed"
            ? "Verification failed"
            : "Unaccepted candidate"}
      </Badge>
      <dl className="rv-key-values">
        <dt>Attempt</dt>
        <dd>
          <code>{result.attempt}</code>
        </dd>
        <dt>Artifact SHA-256</dt>
        <dd>
          <code>{result.sha256}</code>
        </dd>
        <dt>Test evidence</dt>
        <dd>
          {result.tests.length
            ? result.tests.join(", ")
            : "No completed independent tests"}
        </dd>
        <dt>Publication</dt>
        <dd>
          {result.published && result.accepted
            ? "Isolated reference reconciled"
            : "No accepted publication"}
        </dd>
        {result.ref && result.published && (
          <>
            <dt>Review reference</dt>
            <dd>
              <code>{result.ref}</code>
            </dd>
          </>
        )}
      </dl>
    </div>
  );
}
export function ActivityTimeline({ events }: { events: Snapshot["activity"] }) {
  const { date } = useLocale();
  return (
    <ol className="rv-activity" aria-label="Project activity">
      {events.map((e) => (
        <li key={e.id}>
          <span className="rv-activity-symbol">
            <Icon
              name={
                e.kind === "verification"
                  ? "shield"
                  : e.kind === "financial"
                    ? "costs"
                    : e.kind === "publication"
                      ? "github"
                      : "execution"
              }
            />
          </span>
          <div>
            <strong>{e.title}</strong>
            <p>{e.detail}</p>
            <small>
              <time dateTime={e.at}>{date(e.at)} UTC</time> ·{" "}
              <code>{e.id}</code>
            </small>
          </div>
        </li>
      ))}
    </ol>
  );
}
