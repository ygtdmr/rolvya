import { createContext, useContext, useMemo, type ReactNode } from "react";
export type Locale = "en-US" | "en-GB" | "qps-ploc" | "qps-rtl";
export const messages = {
  projects: "Projects",
  execution: "Execution",
  "costs-approvals": "Costs & Approvals",
  connections: "Connections",
  "results-activity": "Results & Activity",
  search: "Search tasks",
  settings: "Display preferences",
  inspector: "Task inspector",
  close: "Close",
  refresh: "Refresh",
  theme: "Theme",
  locale: "Locale",
  unavailable: "Unavailable",
  synthetic: "Synthetic workspace",
  plan: "Immutable plan",
  details: "Inspect task",
};
export type Message = keyof typeof messages;
export function translate(key: Message, locale: Locale): string {
  const value = messages[key];
  return locale.startsWith("qps-")
    ? `[${value.replace(/[aeiou]/g, (c) => ({ a: "á", e: "ë", i: "ï", o: "ø", u: "ü" })[c] ?? c)} ·····]`
    : value;
}
export const numberLocale = (locale: Locale): string =>
  locale.startsWith("qps-") ? "en-US" : locale;
const LocaleContext = createContext<Locale>("en-US");
export function LocaleProvider({
  locale,
  children,
}: {
  locale: Locale;
  children: ReactNode;
}) {
  return <LocaleContext value={locale}>{children}</LocaleContext>;
}
export function useLocale() {
  const locale = useContext(LocaleContext);
  return useMemo(
    () => ({
      locale,
      t: (key: Message) => translate(key, locale),
      date: (at: string) =>
        new Intl.DateTimeFormat(numberLocale(locale), {
          dateStyle: "medium",
          timeStyle: "short",
          timeZone: "UTC",
        }).format(new Date(at)),
      count: (count: number, word: string) =>
        `${new Intl.NumberFormat(numberLocale(locale)).format(count)} ${word}${new Intl.PluralRules(numberLocale(locale)).select(count) === "one" ? "" : "s"}`,
    }),
    [locale],
  );
}
/** Exact integer minor units, never a financial authorization calculation. */
export function formatMoney(
  minor: string | null,
  currency: string,
  locale: Locale,
): string {
  if (minor === null) return "Unknown";
  if (!/^-?(0|[1-9][0-9]{0,18})$/.test(minor) || !/^[A-Z]{3}$/.test(currency))
    return "Unavailable";
  const value = BigInt(minor);
  const abs = value < 0n ? -value : value;
  const formatter = new Intl.NumberFormat(numberLocale(locale), {
    style: "currency",
    currency,
  });
  const digits = formatter.resolvedOptions().maximumFractionDigits ?? 2;
  const scale = 10n ** BigInt(digits);
  const whole = value < 0n ? -(abs / scale) : abs / scale;
  const parts = formatter.formatToParts(whole);
  if (value < 0n && whole === 0n)
    parts.unshift({ type: "minusSign", value: "−" });
  return parts
    .map((p) =>
      p.type === "fraction"
        ? (abs % scale).toString().padStart(digits, "0")
        : p.value,
    )
    .join("");
}
