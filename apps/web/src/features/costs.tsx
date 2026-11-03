import { useState } from "react";
import type { Snapshot } from "../api/model";
import { formatMoney, useLocale } from "../i18n/locale";
import { Button } from "../ui/controls";
import {
  Badge,
  DataTable,
  EmptyState,
  Metric,
  PageHeader,
  Surface,
} from "../ui/data";
import { Dialog } from "../ui/overlays";
import { BudgetMeter, CostBreakdown, CostSummary } from "../ui/patterns";
import { BudgetApproval } from "./budget-approval";
export function Costs({
  snapshot,
  onRefresh = () => undefined,
  commandsEnabled = false,
}: {
  snapshot: Snapshot;
  onRefresh?: () => void;
  commandsEnabled?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const { locale, date } = useLocale();
  const f = snapshot.finance;
  return (
    <div className="rv-section-stack">
      <PageHeader
        section="03 / Financial authority"
        title="Every ceiling is explicit."
        description="Settled charges, reserved exposure and uncertain liabilities remain distinct."
        actions={
          <Button
            icon="lock"
            variant="secondary"
            onClick={() => setOpen(true)}
            disabled={!f}
          >
            {!__DEMO__ && snapshot.capabilities.budgetIncrease
              ? "Increase ceiling"
              : "Preview approval"}
          </Button>
        }
      />
      {!f ? (
        <EmptyState title="Financial view unavailable">
          {snapshot.role === "owner"
            ? "No approved financial projection is recorded for this project."
            : "This read capability requires current authorized Owner access."}{" "}
          No amounts are inferred or shown as zero.
        </EmptyState>
      ) : (
        <>
          {f.paused && (
            <div className="rv-budget-alert">
              <IconlessPause />
              <div>
                <strong>New billable admission is paused</strong>
                <p>
                  {f.recoveryRequired
                    ? "Restore reconciliation is required. Snapshot balances are historical; new spending remains blocked."
                    : f.unknownLiabilities > 0
                      ? "External liability is unresolved. Available headroom cannot be confirmed."
                      : "The controller has paused new billable work."}{" "}
                  Existing accepted work and financial holds are preserved.
                </p>
              </div>
              <Badge tone="caution">Controller gate</Badge>
            </div>
          )}
          <div className="rv-metrics">
            <Metric
              label="Approved ceiling"
              value={formatMoney(f.ceiling, f.currency, locale)}
              note={`Explicit approval · version ${f.version}`}
            />
            <Metric
              label="Reserved exposure"
              value={formatMoney(f.reserved, f.currency, locale)}
              note="Conservative holds, not settled charges"
              tone="accent"
            />
            <Metric
              label="Settled charges"
              value={formatMoney(f.settled, f.currency, locale)}
              note="Recorded financial evidence"
            />
            <Metric
              label="Available headroom"
              value={formatMoney(f.available, f.currency, locale)}
              note={
                f.available === null
                  ? "Unknown liabilities prevent confirmation"
                  : "Authoritative snapshot value"
              }
              tone="caution"
            />
          </div>
          <div className="rv-columns">
            <Surface title="Exposure envelope" eyebrow="HARD BUDGET GATE">
              <CostSummary finance={f} />
              <BudgetMeter finance={f} />
              <dl className="rv-key-values rv-pad">
                <dt>Additional provisional exposure</dt>
                <dd>{formatMoney(f.provisional, f.currency, locale)}</dd>
                <dt>Unresolved liabilities</dt>
                <dd>{f.unknownLiabilities} · never represented as zero</dd>
              </dl>
            </Surface>
            <Surface
              title="Forecast scenarios"
              eyebrow="ESTIMATES / NOT AUTHORITY"
            >
              <div className="rv-forecasts">
                {Object.entries(f.forecasts ?? {}).map(([key, value]) => (
                  <div key={key}>
                    <span>{key}</span>
                    <strong>{formatMoney(value, f.currency, locale)}</strong>
                  </div>
                ))}
              </div>
              {f.forecasts === null && (
                <p className="rv-surface-intro">
                  No authoritative forecast projection is recorded. No estimate
                  is inferred from the ceiling or settled charges.
                </p>
              )}
              <p className="rv-surface-intro">
                Estimates do not authorize spending. A browser timeout or
                successful stop request does not settle external billing.
              </p>
            </Surface>
          </div>
          <Surface title="Four cost categories" eyebrow="SETTLED / RESERVED">
            <CostBreakdown finance={f} />
          </Surface>
          <Surface title="Approval history" eyebrow="DURABLE AUDIT REFERENCES">
            <DataTable
              caption="Budget approval history"
              rowKey={(r) => r.id}
              rows={f.approvals}
              columns={[
                {
                  key: "ref",
                  label: "Evidence",
                  render: (r) => <code>{r.id}</code>,
                },
                { key: "actor", label: "Actor", render: (r) => r.actor },
                {
                  key: "effect",
                  label: "Approved total",
                  render: (r) => (
                    <strong>{formatMoney(r.total, f.currency, locale)}</strong>
                  ),
                },
                { key: "version", label: "Version", render: (r) => r.version },
                { key: "at", label: "Recorded", render: (r) => date(r.at) },
              ]}
            />
          </Surface>
          <Dialog
            open={open}
            onClose={() => setOpen(false)}
            title="Exact-action approval preview"
          >
            <BudgetApproval
              key={snapshot.project.id}
              snapshot={snapshot}
              onRefresh={onRefresh}
              active={commandsEnabled}
            />
          </Dialog>
        </>
      )}
    </div>
  );
}
function IconlessPause() {
  return (
    <span className="rv-pause-mark" aria-hidden="true">
      Ⅱ
    </span>
  );
}
