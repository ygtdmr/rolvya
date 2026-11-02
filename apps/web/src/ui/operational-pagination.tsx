import type { PageResource, Section, Snapshot } from "../api/model";
import { Button } from "./controls";
export function OperationalPagination({
  snapshot,
  section,
  busy,
  load,
}: {
  snapshot: Snapshot;
  section: Section;
  busy: boolean;
  load: (resource: PageResource, cursor: string) => void;
}) {
  const pages = Object.entries(snapshot.pages ?? {}).filter(
    ([resource, cursor]) =>
      cursor !== null &&
      ((section === "execution" && resource === "attempts") ||
        (section === "costs-approvals" && resource === "approvals") ||
        (section === "connections" && resource.startsWith("connections/")) ||
        (section === "results-activity" &&
          (resource === "results" || resource.startsWith("activity/")))),
  );
  if (!pages.length) return null;
  return (
    <div className="rv-section-stack" data-ui="operational-pages">
      <p className="rv-help">
        Bounded source-specific records. Each page is freshly authorized;
        journal sequences are not a global event order.
      </p>
      <div className="rv-filter-bar">
        {pages.map(([resource, cursor]) => (
          <Button
            key={resource}
            disabled={busy}
            loading={busy}
            onClick={() => load(resource as PageResource, cursor!)}
          >
            Load older {resource.replaceAll("/", " · ")}
          </Button>
        ))}
      </div>
    </div>
  );
}
