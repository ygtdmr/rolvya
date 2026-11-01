import { useState } from "react";
import type { Snapshot } from "../api/model";
import { Button, Select } from "../ui/controls";
import {
  Badge,
  EmptyState,
  PageHeader,
  StatusBadge,
  Surface,
} from "../ui/data";
import { AttemptTimeline, WorkerStatus } from "../ui/patterns";
export function Execution({
  snapshot,
  selected,
  onSelect,
}: {
  snapshot: Snapshot;
  selected: string | null;
  onSelect: (id: string) => void;
}) {
  const [filter, setFilter] = useState("all");
  const attempts = snapshot.attempts.filter(
    (a) =>
      (!selected || a.task === selected) &&
      (filter === "all" || a.state === filter),
  );
  return (
    <div className="rv-section-stack">
      <PageHeader
        section="02 / Execution control"
        title="Parallel work, visible authority."
        description="Follow attempts, worker identity and verification without confusing progress with acceptance."
        actions={
          <Badge tone="accent" icon="execution">
            {snapshot.attempts.filter((a) => a.state === "running").length}{" "}
            active worker
          </Badge>
        }
      />
      <div className="rv-filter-bar">
        <Select
          label="Attempt state"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        >
          <option value="all">All attempts</option>
          <option value="running">Running</option>
          <option value="terminated">Terminated</option>
          <option value="unknown">Unknown</option>
          <option value="fenced">Fenced</option>
        </Select>
        {selected && (
          <Button variant="ghost" onClick={() => onSelect("")}>
            Clear task {selected} filter
          </Button>
        )}
      </div>
      {attempts.length === 0 && (
        <EmptyState title="No matching attempts">
          A task may still be planned or blocked. No worker is inferred from a
          task selection.
        </EmptyState>
      )}
      <div className="rv-attempt-grid">
        {attempts.map((a) => {
          const task = snapshot.tasks.find((t) => t.id === a.task);
          return (
            <Surface
              key={a.id}
              className="rv-attempt-card"
              title={task?.title ?? a.task}
              eyebrow={`TASK ${a.task} / ${a.id}`}
              actions={task && <StatusBadge status={task.status} />}
            >
              <WorkerStatus attempt={a} />
              <div className="rv-attempt-phase">
                <strong>{a.phase}</strong>
                <small>
                  Fence {a.epoch} · {a.connection}
                </small>
              </div>
              <AttemptTimeline attempt={a} />
              <details className="rv-log">
                <summary>Redacted execution log</summary>
                <pre>{a.logs.join("\n")}</pre>
              </details>
              <div className="rv-card-footer">
                <Button
                  variant="secondary"
                  onClick={() => onSelect(a.task)}
                  icon="info"
                >
                  Inspect task {a.task}
                </Button>
                <Badge icon="lock">Read-only</Badge>
              </div>
            </Surface>
          );
        })}
      </div>
      <Surface title="Dependency and blocker state">
        <ul className="rv-task-tree">
          {snapshot.tasks.map((t) => (
            <li key={t.id}>
              <button className="rv-task-row" onClick={() => onSelect(t.id)}>
                <span className="rv-task-letter">{t.id}</span>
                <span className="rv-task-copy">
                  <strong>{t.title}</strong>
                  <small>
                    {t.blockers[0] ??
                      (t.kind === "group"
                        ? "Non-executable aggregation"
                        : "No dependency blocker recorded")}
                  </small>
                </span>
                <StatusBadge status={t.status} />
              </button>
            </li>
          ))}
        </ul>
      </Surface>
      <p className="rv-note">
        An interrupted event stream indicates a connectivity gap, not a failed
        worker. Unknown provider execution retains its financial exposure until
        reconciled.
      </p>
    </div>
  );
}
