import type { Locale } from "../i18n/locale";
export interface Preferences {
  theme: "dark" | "light" | "system";
  locale: Locale;
  motion: "system" | "reduce";
}
const defaults: Preferences = {
  theme: "dark",
  locale: "en-US",
  motion: "system",
};
export function loadPreferences(): Preferences {
  try {
    const v = JSON.parse(
      localStorage.getItem("rolvya-display-v1") ?? "null",
    ) as Partial<Preferences> | null;
    return {
      theme:
        v && ["dark", "light", "system"].includes(v.theme ?? "")
          ? v.theme!
          : defaults.theme,
      locale:
        v &&
        [
          "en-US",
          "en-GB",
          ...(__DEMO__ ? ["qps-ploc", "qps-rtl"] : []),
        ].includes(v.locale ?? "")
          ? v.locale!
          : defaults.locale,
      motion: v?.motion === "reduce" ? "reduce" : "system",
    };
  } catch {
    return defaults;
  }
}
export function savePreferences(v: Preferences): void {
  try {
    localStorage.setItem("rolvya-display-v1", JSON.stringify(v));
  } catch {
    /* Display preferences are optional. */
  }
}
