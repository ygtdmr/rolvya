import { useState } from "react";
import type { Snapshot } from "../api/model";
import { Button, Select } from "../ui/controls";
import { Badge, EmptyState, PageHeader } from "../ui/data";
import { Dialog } from "../ui/overlays";
import { ConnectionCard, CredentialModeIndicator } from "../ui/patterns";
export function Connections({ snapshot }: { snapshot: Snapshot }) {
  const [filter, setFilter] = useState("all"),
    [selectedId, setSelected] = useState<string | null>(null);
  // A selection is never a second store of Owner-only account metadata.
  const selected =
    snapshot.connections.find((c) => c.id === selectedId) ?? null;
  const items = snapshot.connections.filter(
    (c) => filter === "all" || c.mode === filter,
  );
  return (
    <div className="rv-section-stack">
      <PageHeader
        section="04 / Account isolation"
        title="Connections, never ambient access."
        description="Every account has its own identity, project grant, access mode and entitlement."
        actions={
          <Button disabled icon="lock" variant="secondary">
            Enrollment unavailable
          </Button>
        }
      />
      <div className="rv-filter-bar">
        <Select
          label="Connection mode"
          value={filter}
          onChange={(next) => setFilter(next)}
          options={[
            { value: "all", label: "All connection modes" },
            { value: "api", label: "API · separately billed" },
            { value: "cli", label: "CLI subscription" },
            { value: "repository", label: "Remote repository" },
            { value: "infrastructure", label: "Infrastructure" },
          ]}
        />
        <Badge icon="shield">Credentials stay in the Secret Broker</Badge>
      </div>
      {items.length === 0 && (
        <EmptyState
          title={
            !snapshot.capabilities.connections
              ? "Connection details unavailable"
              : snapshot.connections.length
                ? "No matching connections"
                : "No registered connections"
          }
        >
          No account enrollment or entitlement is inferred from this view.
        </EmptyState>
      )}
      <div className="rv-connection-grid">
        {items.map((c) => (
          <ConnectionCard
            key={c.id}
            connection={c}
            onInspect={() => setSelected(c.id)}
          />
        ))}
      </div>
      <p className="rv-note">
        CLI login is not proof of permission for hosted subscription execution.
        API access is a distinct, separately billed mode. No silent fallback,
        shared account home or browser credential upload is available.
      </p>
      <Dialog
        open={selected !== null}
        title={selected?.label ?? "Connection authority"}
        onClose={() => setSelected(null)}
      >
        {selected && (
          <div className="rv-section-stack">
            <CredentialModeIndicator connection={selected} />
            <dl className="rv-key-values">
              <dt>Immutable connection</dt>
              <dd>
                <code>{selected.id}</code>
              </dd>
              <dt>Account</dt>
              <dd>{selected.account}</dd>
              <dt>Project grant</dt>
              <dd>
                <code>{selected.scope}</code>
              </dd>
              <dt>Version</dt>
              <dd>{selected.version}</dd>
            </dl>
            <p>{selected.reason}</p>
            <p className="rv-note">
              No secret value is returned to this panel. Connection changes
              require the existing exact-action Owner authority and a verified
              browser adapter.
            </p>
            <Button disabled icon="lock">
              Change requires trusted integration
            </Button>
          </div>
        )}
      </Dialog>
    </div>
  );
}
