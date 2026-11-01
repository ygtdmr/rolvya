import { useState } from "react";
import type { Snapshot } from "../api/model";
import { TaskGraph } from "../graph/TaskGraph";
import { useLocale } from "../i18n/locale";
import { SearchField, Tabs } from "../ui/controls";
import { Badge, Metric, PageHeader, Pagination, Surface } from "../ui/data";
import { ProjectSummary, TaskTree } from "../ui/patterns";
export function Projects({
  snapshot,
  selected,
  onSelect,
}: {
  snapshot: Snapshot;
  selected: string | null;
  onSelect: (id: string) => void;
}) {
  const [view, setView] = useState<"graph" | "list">(() =>
      matchMedia("(max-width: 767px)").matches ? "list" : "graph",
    ),
    [query, setQuery] = useState(""),
    [page, setPage] = useState(0);
  const { t } = useLocale();
  const filtered = snapshot.tasks.filter((task) =>
    `${task.id} ${task.title} ${task.status}`
      .toLowerCase()
      .includes(query.toLowerCase()),
  );
  const effectivePage = Math.min(
    page,
    Math.max(0, Math.ceil(filtered.length / 50) - 1),
  );
  return (
    <div className="rv-section-stack">
      <PageHeader
        section="01 / Project control"
        title={snapshot.project.name}
        description="One frozen plan. Parallel work. Independently verified outcomes."
        actions={
          <Badge icon="lock" tone="accent">
            Plan v{snapshot.plan.version} · immutable
          </Badge>
        }
      />
      <ProjectSummary snapshot={snapshot} />
      <div className="rv-metrics">
        <Metric
          label="Executable tasks"
          value={snapshot.tasks.filter((x) => x.kind === "leaf").length}
          note="Only leaves can receive attempts"
        />
        <Metric
          label="Running now"
          value={snapshot.tasks.filter((x) => x.status === "running").length}
          note="Across isolated worker identities"
          tone="accent"
        />
        <Metric
          label="Independently verified"
          value={
            snapshot.tasks.filter(
              (x) => x.kind === "leaf" && x.status === "verified",
            ).length
          }
          note="Accepted evidence, not process exit"
          tone="positive"
        />
        <Metric
          label="Blocked"
          value={snapshot.tasks.filter((x) => x.status === "blocked").length}
          note="Waiting on accepted predecessors"
        />
      </div>
      <Surface
        title="Plan topology"
        eyebrow="CONNECTED CONTROL"
        actions={
          <Tabs
            panelId="plan-view"
            label="Plan representations"
            value={view}
            onChange={setView}
            options={[
              { value: "graph", label: "Dependency graph" },
              { value: "list", label: "Task hierarchy" },
            ]}
          />
        }
      >
        <div
          role="tabpanel"
          id="plan-view"
          aria-labelledby={`plan-view-tab-${view}`}
        >
          <p className="rv-surface-intro">
            Dependencies determine eligibility. Groups organize tasks and never
            execute.
          </p>
          {view === "graph" && (
            <TaskGraph
              tasks={snapshot.tasks}
              selected={selected}
              onSelect={onSelect}
            />
          )}
          <div className="rv-task-list-heading">
            <h3>
              {view === "graph"
                ? "Keyboard-accessible task hierarchy"
                : "Task hierarchy"}
            </h3>
            <SearchField
              label={t("search")}
              placeholder="Name, ID or status…"
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                setPage(0);
              }}
            />
          </div>
          <TaskTree
            tasks={filtered.slice(effectivePage * 50, (effectivePage + 1) * 50)}
            selected={selected}
            onSelect={onSelect}
          />
          {filtered.length > 50 && (
            <Pagination
              total={filtered.length}
              page={effectivePage}
              size={50}
              onChange={setPage}
            />
          )}
        </div>
      </Surface>
      <div className="rv-columns">
        <Surface title="Independent by design" eyebrow="EXECUTION BOUNDARY">
          <p className="rv-surface-intro">
            Independent leaves may run in parallel. Dependent tasks wait for
            their declared accepted predecessor evidence. Worker termination
            alone does not unblock a dependency.
          </p>
          <div className="rv-inline-facts">
            <Badge icon="shield">Independent verification</Badge>
            <Badge icon="github">Isolated publication refs</Badge>
          </div>
        </Surface>
        <Surface title="Authority stays explicit" eyebrow="CONTROL BOUNDARY">
          <p className="rv-surface-intro">
            This panel can inspect the approved plan. Worker creation, financial
            approval and repository writes remain server-authorized operations.
          </p>
          <div className="rv-inline-facts">
            <Badge icon="lock">No DAG mutation</Badge>
            <Badge icon="costs">Hard budget gate</Badge>
          </div>
        </Surface>
      </div>
    </div>
  );
}
