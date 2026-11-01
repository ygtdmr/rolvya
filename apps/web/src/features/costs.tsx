import { useState } from "react";
import type { Snapshot } from "../api/model";
import { formatMoney, useLocale } from "../i18n/locale";
import { Button, TextField } from "../ui/controls";
import {
  Badge,
  DataTable,
  EmptyState,
  Metric,
  PageHeader,
  Surface,
} from "../ui/data";
import { Dialog } from "../ui/overlays";
import {
  ApprovalPreview,
  BudgetMeter,
  CostBreakdown,
  CostSummary,
} from "../ui/patterns";
export function Costs({ snapshot }: { snapshot: Snapshot }) {
  const [open, setOpen] = useState(false),
    [total, setTotal] = useState("30000");
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
            Preview approval
          </Button>
        }
      />
      {!f ? (
        <EmptyState title="Financial view unavailable">
          This read capability requires current authorized Owner access. No
          amounts are inferred or shown as zero.
        </EmptyState>
      ) : (
        <>
          {f.paused && (
            <div className="rv-budget-alert">
              <IconlessPause />
              <div>
                <strong>New billable admission is paused</strong>
                <p>
                  {f.unknownLiabilities > 0
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
                {Object.entries(f.forecasts).map(([key, value]) => (
                  <div key={key}>
                    <span>{key}</span>
                    <strong>{formatMoney(value, f.currency, locale)}</strong>
                  </div>
                ))}
              </div>
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
            <div className="rv-section-stack">
              <TextField
                label="Proposed total · integer minor units"
                inputMode="numeric"
                value={total}
                onChange={(e) => setTotal(e.target.value)}
                error={
                  !/^(0|[1-9][0-9]{0,18})$/.test(total)
                    ? "Enter bounded integer minor units."
                    : undefined
                }
              />
              <ApprovalPreview
                finance={f}
                project={snapshot.project.id}
                total={total}
              />
              <p className="rv-note">
                Submission is unavailable. The browser command adapter must
                verify a current Owner, fresh action-bound proof and exact
                versions atomically with the financial effect.
              </p>
              <Button disabled icon="lock">
                Submit requires verified Owner adapter
              </Button>
            </div>
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
