import {
  cloneElement,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ReactElement,
  type ReactNode,
} from "react";
import { Button, IconButton, type IconName } from "./controls";
const focusables =
  'button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled),a[href],[tabindex="0"]';
export function Dialog({
  open,
  onClose,
  title,
  children,
  drawer = false,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
  drawer?: boolean;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const close = useRef(onClose);
  const titleId = useId();
  useLayoutEffect(() => {
    close.current = onClose;
  }, [onClose]);
  useEffect(() => {
    const el = ref.current;
    if (!el || !open) return;
    const previous =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    if (typeof el.showModal === "function") el.showModal();
    else el.setAttribute("open", "");
    el.querySelector<HTMLElement>(focusables)?.focus();
    return () => {
      if (el.open && typeof el.close === "function") el.close();
      else el.removeAttribute("open");
      (previous?.isConnected
        ? previous
        : document.querySelector<HTMLElement>("[data-page-heading]")
      )?.focus();
    };
  }, [open]);
  return (
    <dialog
      ref={ref}
      aria-modal="true"
      aria-labelledby={titleId}
      className={`rv-dialog ${drawer ? "rv-drawer" : ""}`}
      data-ui={drawer ? "drawer" : "dialog"}
      onCancel={(e) => {
        e.preventDefault();
        close.current();
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget) close.current();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          close.current();
        }
        if (e.key === "Tab") {
          const items = [
            ...e.currentTarget.querySelectorAll<HTMLElement>(focusables),
          ].filter((x) => !x.hidden && !x.closest("[hidden]"));
          const first = items[0],
            last = items.at(-1);
          if (!first) {
            e.preventDefault();
            e.currentTarget.focus();
          } else if (e.shiftKey && document.activeElement === first) {
            e.preventDefault();
            last?.focus();
          } else if (!e.shiftKey && document.activeElement === last) {
            e.preventDefault();
            first.focus();
          }
        }
      }}
    >
      <div className="rv-dialog-surface">
        <header className="rv-dialog-header">
          <h2 id={titleId}>{title}</h2>
          <IconButton
            icon="close"
            label="Close"
            variant="ghost"
            onClick={onClose}
          />
        </header>
        <div className="rv-dialog-content">{open && children}</div>
      </div>
    </dialog>
  );
}
export function Drawer(props: Omit<Parameters<typeof Dialog>[0], "drawer">) {
  return <Dialog {...props} drawer />;
}
export function Popover({
  label,
  text = label,
  icon = "settings",
  children,
}: {
  label: string;
  text?: string;
  icon?: IconName;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const id = useId();
  useEffect(() => {
    if (!open) return;
    ref.current
      ?.querySelector<HTMLElement>(
        "[data-popover-panel] " +
          focusables.split(",").join(",[data-popover-panel] "),
      )
      ?.focus();
    const outside = (event: PointerEvent) => {
      if (event.target instanceof Node && !ref.current?.contains(event.target))
        setOpen(false);
    };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [open]);
  return (
    <div
      ref={ref}
      className="rv-popover"
      data-ui="popover"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          setOpen(false);
          trigger.current?.focus();
        }
      }}
    >
      <Button
        ref={trigger}
        icon={icon}
        variant="ghost"
        aria-expanded={open}
        aria-controls={id}
        aria-label={label}
        aria-haspopup="dialog"
        onClick={() => setOpen(!open)}
      >
        {text}
      </Button>
      {open && (
        <div
          id={id}
          className="rv-popover-panel"
          data-popover-panel
          role="dialog"
          aria-label={label}
        >
          {children}
          <Button
            variant="ghost"
            onClick={() => {
              setOpen(false);
              trigger.current?.focus();
            }}
          >
            Done
          </Button>
        </div>
      )}
    </div>
  );
}
export function Menu({
  label,
  items,
}: {
  label: string;
  items: { label: string; onSelect: () => void; disabled?: boolean }[];
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const enabled = () => [
    ...(ref.current?.querySelectorAll<HTMLButtonElement>(
      "[role=menuitem]:not(:disabled)",
    ) ?? []),
  ];
  useEffect(() => {
    if (open)
      ref.current
        ?.querySelector<HTMLButtonElement>("[role=menuitem]:not(:disabled)")
        ?.focus();
  }, [open]);
  useEffect(() => {
    if (!open) return;
    const listener = (e: PointerEvent) => {
      if (e.target instanceof Node && !ref.current?.contains(e.target))
        setOpen(false);
    };
    document.addEventListener("pointerdown", listener);
    return () => document.removeEventListener("pointerdown", listener);
  }, [open]);
  const close = () => {
    setOpen(false);
    trigger.current?.focus();
  };
  return (
    <div
      ref={ref}
      className="rv-popover"
      data-ui="menu"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          close();
        }
        if (e.key === "Tab") setOpen(false);
        const buttons = enabled();
        const index = buttons.indexOf(
          document.activeElement as HTMLButtonElement,
        );
        const next =
          e.key === "ArrowDown"
            ? (index + 1) % buttons.length
            : e.key === "ArrowUp"
              ? (index - 1 + buttons.length) % buttons.length
              : e.key === "Home"
                ? 0
                : e.key === "End"
                  ? buttons.length - 1
                  : -1;
        if (next >= 0) {
          e.preventDefault();
          if (!open) setOpen(true);
          else buttons[next]?.focus();
        }
      }}
    >
      <Button
        ref={trigger}
        variant="ghost"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        {label}
      </Button>
      {open && (
        <div
          className="rv-popover-panel rv-menu"
          role="menu"
          aria-label={label}
        >
          {items.map((item) => (
            <button
              key={item.label}
              type="button"
              role="menuitem"
              disabled={item.disabled}
              onClick={() => {
                close();
                item.onSelect();
              }}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
export function Tooltip({
  label,
  children,
}: {
  label: string;
  children: ReactElement<{ "aria-describedby"?: string | undefined }>;
}) {
  const [shown, setShown] = useState(false);
  const id = useId();
  return (
    <span
      className="rv-tooltip-wrap"
      onMouseEnter={() => setShown(true)}
      onMouseLeave={() => setShown(false)}
      onFocus={() => setShown(true)}
      onBlur={() => setShown(false)}
      onKeyDown={(e) => {
        if (e.key === "Escape") setShown(false);
      }}
    >
      {cloneElement(children, {
        "aria-describedby":
          [children.props["aria-describedby"], shown ? id : ""]
            .filter(Boolean)
            .join(" ") || undefined,
      })}
      {shown && (
        <span className="rv-tooltip" id={id} role="tooltip">
          {label}
        </span>
      )}
    </span>
  );
}
export function Toast({
  message,
  onDismiss,
}: {
  message: string;
  onDismiss: () => void;
}) {
  return (
    <div className="rv-toast" data-ui="toast">
      <span role="status">{message}</span>
      <IconButton
        label="Dismiss notification"
        icon="close"
        variant="ghost"
        onClick={onDismiss}
      />
    </div>
  );
}
