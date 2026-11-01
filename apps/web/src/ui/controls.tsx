import {
  forwardRef,
  useId,
  type ButtonHTMLAttributes,
  type InputHTMLAttributes,
  type SelectHTMLAttributes,
  type ReactNode,
} from "react";
const ICONS = {
  projects: "panels-top-left",
  execution: "workflow",
  costs: "wallet",
  connections: "cable",
  results: "list-checks",
  search: "search",
  close: "x",
  menu: "menu",
  settings: "sliders-horizontal",
  chevron: "chevron-right",
  external: "arrow-up-right",
  copy: "copy",
  refresh: "refresh-cw",
  plus: "plus",
  minus: "minus",
  fit: "scan",
  check: "check",
  clock: "clock-3",
  info: "info",
  github: "git-branch",
  lock: "lock-keyhole",
  layers: "layers-3",
  sun: "sun",
  moon: "moon",
  filter: "list-filter",
  arrow: "arrow-right",
  shield: "shield-check",
} as const;
export type IconName = keyof typeof ICONS;
export function Icon({ name }: { name: IconName }) {
  return (
    <span className="rv-icon" aria-hidden="true">
      <img
        alt=""
        width="18"
        height="18"
        crossOrigin="anonymous"
        referrerPolicy="no-referrer"
        src={`https://cdn.jsdelivr.net/npm/lucide-static@1.66.0/icons/${ICONS[name]}.svg`}
        onError={(e) => {
          e.currentTarget.hidden = true;
        }}
      />
    </span>
  );
}
type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  loading?: boolean;
  icon?: IconName;
};
export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  function Button(
    {
      variant = "secondary",
      loading = false,
      icon,
      disabled,
      children,
      type = "button",
      ...props
    },
    ref,
  ) {
    return (
      <button
        {...props}
        ref={ref}
        type={type}
        className={`rv-button rv-button--${variant} ${props.className ?? ""}`}
        disabled={disabled || loading}
        aria-busy={loading || undefined}
        data-ui="button"
      >
        {icon && <Icon name={icon} />}
        {loading ? "Working…" : children}
      </button>
    );
  },
);
export function IconButton({
  label,
  icon,
  text = label,
  ...props
}: Omit<ButtonProps, "children"> & {
  label: string;
  icon: IconName;
  text?: string;
}) {
  return (
    <Button
      {...props}
      icon={icon}
      className={`rv-icon-button ${props.className ?? ""}`}
      aria-label={label}
    >
      <span>{text}</span>
    </Button>
  );
}
type FieldProps = InputHTMLAttributes<HTMLInputElement> & {
  label: string;
  hint?: string | undefined;
  error?: string | undefined;
};
export function TextField({
  label,
  hint,
  error,
  id: passed,
  ...props
}: FieldProps) {
  const generated = useId();
  const id = passed ?? generated;
  return (
    <div className="rv-field" data-ui="text-field">
      <label htmlFor={id}>{label}</label>
      <input
        {...props}
        id={id}
        aria-invalid={Boolean(error)}
        aria-describedby={error || hint ? `${id}-help` : undefined}
      />
      {(error || hint) && (
        <span
          id={`${id}-help`}
          className={error ? "rv-field-error" : "rv-help"}
        >
          {error ?? hint}
        </span>
      )}
    </div>
  );
}
export function SearchField(props: FieldProps) {
  return (
    <div className="rv-search" data-ui="search">
      <Icon name="search" />
      <TextField {...props} type="search" />
    </div>
  );
}
export function Select({
  label,
  children,
  id: supplied,
  ...props
}: SelectHTMLAttributes<HTMLSelectElement> & {
  label: string;
  children: ReactNode;
}) {
  const generated = useId();
  const id = supplied ?? generated;
  return (
    <div className="rv-field" data-ui="select">
      <label htmlFor={id}>{label}</label>
      <select {...props} id={id}>
        {children}
      </select>
    </div>
  );
}
export function Checkbox({
  label,
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { label: string }) {
  return (
    <label className="rv-check">
      <input {...props} type="checkbox" data-ui="checkbox" />
      <span>{label}</span>
    </label>
  );
}
export function Switch({
  label,
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { label: string }) {
  return (
    <label className="rv-switch">
      <input {...props} type="checkbox" role="switch" data-ui="switch" />
      <span>{label}</span>
    </label>
  );
}
export function Tabs<T extends string>({
  label,
  value,
  options,
  onChange,
  panelId,
}: {
  label: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
  panelId: string;
}) {
  return (
    <div
      className="rv-tabs"
      role="tablist"
      aria-label={label}
      data-ui="tabs"
      onKeyDown={(event) => {
        const index = options.findIndex((o) => o.value === value);
        const rtl =
          event.currentTarget.closest("[dir]")?.getAttribute("dir") === "rtl";
        const next =
          event.key === (rtl ? "ArrowLeft" : "ArrowRight")
            ? (index + 1) % options.length
            : event.key === (rtl ? "ArrowRight" : "ArrowLeft")
              ? (index - 1 + options.length) % options.length
              : event.key === "Home"
                ? 0
                : event.key === "End"
                  ? options.length - 1
                  : -1;
        if (next >= 0) {
          event.preventDefault();
          const option = options[next];
          if (option) {
            onChange(option.value);
            (event.currentTarget.children[next] as HTMLElement).focus();
          }
        }
      }}
    >
      {options.map((o) => (
        <button
          type="button"
          key={o.value}
          role="tab"
          id={`${panelId}-tab-${o.value}`}
          aria-controls={panelId}
          aria-selected={value === o.value}
          tabIndex={value === o.value ? 0 : -1}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
