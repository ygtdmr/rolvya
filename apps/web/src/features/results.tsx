import { useState } from "react";
import type { Snapshot } from "../api/model";
import { Button, Select, Tabs } from "../ui/controls";
import { Badge, EmptyState, PageHeader, Pagination, Surface } from "../ui/data";
import { ActivityTimeline, ResultEvidence } from "../ui/patterns";
export function Results({
  snapshot,
  selected,
  onSelect,
}: {
  snapshot: Snapshot;
  selected: string | null;
  onSelect: (id: string) => void;
}) {
  const [view, setView] = useState<"results" | "activity">("results"),
    [filter, setFilter] = useState("all"),
    [page, setPage] = useState(0);
  const results = snapshot.results.filter(
    (r) =>
      (!selected || r.task === selected) &&
      (filter === "all" || (filter === "accepted" ? r.accepted : !r.accepted)),
  );
  const events = snapshot.activity.filter(
    (e) => !selected || e.task === selected,
  );
  const items = view === "results" ? results : events;
  const effectivePage = Math.min(
    page,
    Math.max(0, Math.ceil(items.length / 4) - 1),
  );
  return (
    <div className="rv-section-stack">
      <PageHeader
        section="05 / Evidence and provenance"
        title="Outcomes you can inspect."
        description="Independent acceptance and publication have separate evidence. A candidate is never presented as verified."
        actions={<Badge icon="shield">R09 acceptance boundary</Badge>}
      />
      <div className="rv-filter-bar">
        <Tabs
          panelId="evidence-view"
          label="Evidence views"
          value={view}
          onChange={(v) => {
            setView(v);
            setPage(0);
          }}
          options={[
            { value: "results", label: "Results" },
            { value: "activity", label: "Activity" },
          ]}
        />
        <Select
          label="Acceptance"
          value={filter}
          disabled={view === "activity"}
          onChange={(e) => {
            setFilter(e.target.value);
            setPage(0);
          }}
        >
          <option value="all">All evidence</option>
          <option value="accepted">Independently accepted</option>
          <option value="candidate">Unaccepted candidates</option>
        </Select>
        {selected && (
          <Button
            variant="ghost"
            onClick={() => {
              onSelect("");
              setPage(0);
            }}
          >
            Clear task {selected} filter
          </Button>
        )}
      </div>
      <div
        role="tabpanel"
        id="evidence-view"
        aria-labelledby={`evidence-view-tab-${view}`}
        className="rv-section-stack"
      >
        {items.length === 0 && (
          <EmptyState title="No matching evidence">
            No verified output or publication is inferred. Change the filters or
            wait for authoritative records.
          </EmptyState>
        )}
        {view === "results" ? (
          <div className="rv-results-grid">
            {results
              .slice(effectivePage * 4, (effectivePage + 1) * 4)
              .map((r) => (
                <Surface
                  key={r.id}
                  title={
                    snapshot.tasks.find((t) => t.id === r.task)?.title ?? r.task
                  }
                  eyebrow={`TASK ${r.task} / ${r.id}`}
                  actions={
                    <Button variant="ghost" onClick={() => onSelect(r.task)}>
                      Inspect {r.task}
                    </Button>
                  }
                >
                  <ResultEvidence result={r} />
                  {snapshot.provenance === "demo" && (
                    <p className="rv-fixture-note">
                      Synthetic evidence and simulated remote reference. No real
                      repository was modified.
                    </p>
                  )}
                </Surface>
              ))}
          </div>
        ) : (
          <Surface title="Project activity">
            <ActivityTimeline
              events={events.slice(effectivePage * 4, (effectivePage + 1) * 4)}
            />
          </Surface>
        )}
        <Pagination
          total={items.length}
          size={4}
          page={effectivePage}
          onChange={setPage}
        />
      </div>
    </div>
  );
}
