import {
  forwardRef,
  useId,
  useEffect,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type InputHTMLAttributes,
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
export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}
/** Select-only combobox. No native select shadow styles or inline style writes. */
export function Select({
  label,
  options,
  value,
  onChange,
  id: supplied,
  disabled = false,
  loading = false,
  readOnly = false,
  error,
  hint,
}: {
  label: string;
  options: readonly SelectOption[];
  value: string;
  onChange: (value: string) => void;
  id?: string;
  disabled?: boolean;
  loading?: boolean;
  readOnly?: boolean;
  error?: string;
  hint?: string;
}) {
  const generated = useId(),
    id = supplied ?? generated;
  const [open, setOpen] = useState(false),
    [active, setActive] = useState(0);
  const root = useRef<HTMLDivElement>(null),
    trigger = useRef<HTMLButtonElement>(null);
  const search = useRef(""),
    timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const selected = options.findIndex((o) => o.value === value),
    blocked = disabled || loading || readOnly || options.length === 0;
  useEffect(() => {
    if (blocked) setOpen(false);
  }, [blocked]);
  useEffect(() => () => clearTimeout(timer.current), []);
  useEffect(() => {
    if (!open) return;
    const outside = (e: PointerEvent) => {
      if (e.target instanceof Node && !root.current?.contains(e.target))
        setOpen(false);
    };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [open]);
  useEffect(() => {
    if (open)
      document
        .getElementById(`${id}-option-${active}`)
        ?.scrollIntoView?.({ block: "nearest" });
  }, [open, active, id]);
  const enabled = options
    .map((o, i) => (o.disabled ? -1 : i))
    .filter((i) => i >= 0);
  const show = () => {
    if (!blocked && enabled.length) {
      setActive(
        selected >= 0 && !options[selected]?.disabled ? selected : enabled[0]!,
      );
      setOpen(true);
    }
  };
  const choose = (index: number) => {
    const option = options[index];
    if (!blocked && option && !option.disabled) {
      onChange(option.value);
      setOpen(false);
      trigger.current?.focus();
    }
  };
  return (
    <div
      ref={root}
      className="rv-field rv-select"
      data-ui="select"
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget)) setOpen(false);
      }}
    >
      <label id={`${id}-label`} htmlFor={id}>
        {label}
      </label>
      <button
        ref={trigger}
        id={id}
        type="button"
        role="combobox"
        value={value}
        disabled={disabled || loading || options.length === 0}
        aria-readonly={readOnly || undefined}
        aria-busy={loading || undefined}
        aria-invalid={Boolean(error)}
        aria-labelledby={`${id}-label`}
        aria-describedby={error || hint ? `${id}-help` : undefined}
        aria-expanded={open}
        aria-haspopup="listbox"
        aria-controls={`${id}-list`}
        aria-activedescendant={open ? `${id}-option-${active}` : undefined}
        className="rv-select-trigger"
        onClick={() => (open ? setOpen(false) : show())}
        onKeyDown={(e) => {
          if (blocked) return;
          if (e.key === "Escape") {
            if (open) {
              e.preventDefault();
              e.stopPropagation();
              setOpen(false);
            }
            return;
          }
          if (e.key === "Tab") {
            setOpen(false);
            return;
          }
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            if (open) choose(active);
            else show();
            return;
          }
          if (["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) {
            e.preventDefault();
            if (!enabled.length) return;
            const current = open ? active : selected,
              index = enabled.indexOf(current);
            const next =
              e.key === "Home"
                ? enabled[0]
                : e.key === "End"
                  ? enabled.at(-1)
                  : enabled[
                      (index +
                        (e.key === "ArrowDown" ? 1 : -1) +
                        enabled.length) %
                        enabled.length
                    ];
            if (next !== undefined) {
              setActive(next);
              setOpen(true);
            }
            return;
          }
          if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
            e.preventDefault();
            search.current += e.key.toLocaleLowerCase();
            clearTimeout(timer.current);
            timer.current = setTimeout(() => {
              search.current = "";
            }, 600);
            const found = options.findIndex(
              (o) =>
                !o.disabled &&
                o.label.toLocaleLowerCase().startsWith(search.current),
            );
            if (found >= 0) {
              setActive(found);
              setOpen(true);
            }
          }
        }}
      >
        <span>
          {loading
            ? "Loading…"
            : (options[selected]?.label ?? "Choose an option")}
        </span>
        <Icon name="chevron" />
      </button>
      <ul
        hidden={!open}
        id={`${id}-list`}
        role="listbox"
        aria-labelledby={`${id}-label`}
        className="rv-listbox"
      >
        {options.map((o, i) => (
          <li
            id={`${id}-option-${i}`}
            key={o.value}
            role="option"
            aria-selected={value === o.value}
            aria-disabled={o.disabled || undefined}
            data-active={active === i}
            onPointerDown={(e) => e.preventDefault()}
            onPointerMove={() => {
              if (!o.disabled) setActive(i);
            }}
            onClick={() => choose(i)}
          >
            <span>{o.label}</span>
            <small aria-hidden="true">
              {value === o.value ? "Selected" : ""}
            </small>
          </li>
        ))}
      </ul>
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
