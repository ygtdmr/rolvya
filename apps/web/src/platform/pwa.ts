export interface ShellUpdate {
  available: boolean;
  apply: () => void;
}
export function registerShell(
  onUpdate: (value: ShellUpdate) => void,
): () => void {
  if (
    !window.isSecureContext ||
    !("serviceWorker" in navigator) ||
    !navigator.serviceWorker
  )
    return () => undefined;
  let cancelled = false,
    registration: ServiceWorkerRegistration | undefined,
    reloading = false,
    applying = false;
  const watches: ServiceWorker[] = [];
  const changed = () => {
    if (applying && !reloading) {
      reloading = true;
      window.location.reload();
    }
  };
  const check = () => {
    if (
      !cancelled &&
      registration?.waiting &&
      navigator.serviceWorker.controller
    )
      onUpdate({
        available: true,
        apply: () => {
          applying = true;
          registration?.waiting?.postMessage({ type: "ACTIVATE" });
        },
      });
  };
  const installing = () => {
    const worker = registration?.installing;
    if (worker) {
      watches.push(worker);
      worker.addEventListener("statechange", check);
    }
  };
  navigator.serviceWorker.addEventListener("controllerchange", changed);
  void navigator.serviceWorker
    .register("/sw.js", { scope: "/", updateViaCache: "none" })
    .then((r) => {
      if (cancelled) return;
      registration = r;
      r.addEventListener("updatefound", installing);
      check();
      installing();
    })
    .catch(() => {
      /* A shell-cache failure cannot change authentication or application authority. */
    });
  return () => {
    cancelled = true;
    registration?.removeEventListener("updatefound", installing);
    watches.forEach((w) => w.removeEventListener("statechange", check));
    navigator.serviceWorker.removeEventListener("controllerchange", changed);
  };
}
