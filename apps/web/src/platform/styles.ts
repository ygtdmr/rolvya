import css from "virtual:rolvya-styles";
/** Exact text is authorized by the build-derived CSP hash, never unsafe-inline. */
export function mountStyles() {
  const style = document.createElement("style");
  style.dataset.rolvyaStyle = "v1";
  style.textContent = css;
  document.head.append(style);
}
