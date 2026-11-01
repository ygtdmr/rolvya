/** Progressive, display-only transition. Never wraps a privileged operation. */
export function transitionView(update: () => void): void {
  if (
    typeof document.startViewTransition !== "function" ||
    matchMedia("(prefers-reduced-motion: reduce)").matches ||
    document.documentElement.dataset.motion === "reduce"
  ) {
    update();
    return;
  }
  let called = false;
  const once = () => {
    if (!called) {
      called = true;
      update();
    }
  };
  try {
    const view = document.startViewTransition(once);
    void view.finished.catch(() => undefined);
  } catch {
    once();
  }
}
