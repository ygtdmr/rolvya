import type { ReactNode } from "react";
import type { Availability, TaskStatus } from "../api/model";
import { statusLabels } from "../api/model";
import { Button, Icon, type IconName } from "./controls";
export type Tone = "neutral" | "accent" | "positive" | "caution" | "danger";
export const statusTone: Record<TaskStatus, Tone> = {
  planned: "neutral",
  ready: "accent",
  blocked: "neutral",
  running: "accent",
  awaiting_verification: "caution",
  verified: "positive",
  failed: "danger",
  paused: "caution",
  unknown: "caution",
};
export function Badge({
  children,
  tone = "neutral",
  icon,
}: {
  children: ReactNode;
  tone?: Tone;
  icon?: IconName;
}) {
  return (
    <span className="rv-badge" data-tone={tone} data-ui="badge">
      {icon && <Icon name={icon} />}
      {children}
    </span>
  );
}
export function StatusBadge({ status }: { status: TaskStatus }) {
  return (
    <Badge
      tone={statusTone[status]}
      icon={
        status === "verified"
          ? "check"
          : status === "running"
            ? "clock"
            : status === "blocked"
              ? "lock"
              : "info"
      }
    >
      {statusLabels[status]}
    </Badge>
  );
}
export function Surface({
  title,
  eyebrow,
  actions,
  children,
  className = "",
}: {
  title?: string;
  eyebrow?: string;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={`rv-surface ${className}`} data-ui="surface">
      {(title || actions) && (
        <header className="rv-surface-header">
          <div>
            {eyebrow && <p className="rv-eyebrow">{eyebrow}</p>}
            {title && <h2>{title}</h2>}
          </div>
          {actions}
        </header>
      )}
      {children}
    </section>
  );
}
export function Metric({
  label,
  value,
  note,
  tone = "neutral",
}: {
  label: string;
  value: ReactNode;
  note: string;
  tone?: Tone;
}) {
  return (
    <div className="rv-metric" data-tone={tone}>
      <span>{label}</span>
      <strong>{value}</strong>
      <small>{note}</small>
    </div>
  );
}
export function Progress({ label, value }: { label: string; value?: number }) {
  const bounded =
    value === undefined ? undefined : Math.min(100, Math.max(0, value));
  return (
    <div className="rv-progress">
      <span>{label}</span>
      {bounded === undefined ? (
        <span>Unknown progress</span>
      ) : (
        <progress aria-label={label} max={100} value={bounded}>
          {bounded}%
        </progress>
      )}
    </div>
  );
}
export function Skeleton({
  label = "Loading authorized data",
}: {
  label?: string;
}) {
  return (
    <div
      className="rv-skeleton"
      role="status"
      aria-busy="true"
      data-ui="skeleton"
    >
      <span>{label}</span>
      <div />
      <div />
    </div>
  );
}
export function EmptyState({
  title,
  children,
  action,
}: {
  title: string;
  children: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="rv-empty" data-ui="empty">
      <Icon name="layers" />
      <h2>{title}</h2>
      <p>{children}</p>
      {action}
    </div>
  );
}
export function ErrorState({
  title,
  children,
  retry,
}: {
  title: string;
  children: ReactNode;
  retry?: () => void;
}) {
  return (
    <div className="rv-error" role="alert" data-ui="error">
      <h2>{title}</h2>
      <p>{children}</p>
      {retry && (
        <Button onClick={retry} icon="refresh">
          Try again
        </Button>
      )}
    </div>
  );
}
export function ConnectivityBanner({
  state,
  asOf,
  onRefresh,
}: {
  state: Availability;
  asOf: string | null;
  onRefresh: () => void;
}) {
  const copy: Record<Availability, string> = {
    live: "Snapshot current",
    connecting: "Connecting to the controller",
    stale: "Updates interrupted. Displayed data may be stale.",
    offline: "Offline. Current project state cannot be confirmed.",
    unauthorized: "A current human session is required.",
    forbidden: "This project is not available to your account.",
    unavailable: "The authenticated browser adapter is unavailable.",
  };
  return (
    <div
      className="rv-connectivity"
      data-state={state}
      data-ui="connectivity-banner"
    >
      <span role="status">
        <span className="rv-signal" />
        {copy[state]}
      </span>
      <div>
        {asOf && (
          <small>
            Snapshot{" "}
            <time dateTime={asOf}>
              {new Date(asOf).toISOString().slice(11, 16)} UTC
            </time>
          </small>
        )}
        <Button variant="ghost" icon="refresh" onClick={onRefresh}>
          Refresh
        </Button>
      </div>
    </div>
  );
}
export interface Column<T> {
  key: string;
  label: string;
  render: (row: T) => ReactNode;
}
export function DataTable<T>({
  caption,
  columns,
  rows,
  rowKey,
  empty = "No matching records.",
}: {
  caption: string;
  columns: Column<T>[];
  rows: T[];
  rowKey: (row: T) => string;
  empty?: string;
}) {
  return (
    <div className="rv-table-wrap" data-ui="data-table">
      <table>
        <caption className="sr-only">{caption}</caption>
        <thead>
          <tr>
            {columns.map((c) => (
              <th key={c.key} scope="col">
                {c.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={rowKey(row)}>
              {columns.map((c) => (
                <td key={c.key} data-label={c.label}>
                  {c.render(row)}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      {rows.length === 0 && <p className="rv-table-empty">{empty}</p>}
    </div>
  );
}
export function PageHeader({
  section,
  title,
  description,
  actions,
}: {
  section: string;
  title: string;
  description: string;
  actions?: ReactNode;
}) {
  return (
    <header className="rv-page-header">
      <div>
        <p className="rv-eyebrow">{section}</p>
        <h1 tabIndex={-1} data-page-heading>
          {title}
        </h1>
        <p>{description}</p>
      </div>
      <div className="rv-header-actions">{actions}</div>
    </header>
  );
}
export function Pagination({
  page,
  total,
  size,
  onChange,
}: {
  page: number;
  total: number;
  size: number;
  onChange: (page: number) => void;
}) {
  const pages = Math.max(1, Math.ceil(total / size));
  return (
    <nav className="rv-pagination" aria-label="Record pages">
      <span aria-live="polite">
        {total === 0
          ? "No records"
          : `${page * size + 1}–${Math.min((page + 1) * size, total)} of ${total}`}
      </span>
      <div>
        <Button disabled={page === 0} onClick={() => onChange(page - 1)}>
          Previous
        </Button>
        <Button disabled={page + 1 >= pages} onClick={() => onChange(page + 1)}>
          Next
        </Button>
      </div>
    </nav>
  );
}
