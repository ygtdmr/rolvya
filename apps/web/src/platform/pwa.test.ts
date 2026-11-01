import { afterEach, expect, it, vi } from "vitest";
import { registerShell } from "./pwa";
afterEach(() => vi.unstubAllGlobals());
it("does not register on insecure origins or missing support", () => {
  const register = vi.fn();
  vi.stubGlobal("window", { isSecureContext: false });
  vi.stubGlobal("navigator", { serviceWorker: { register } });
  registerShell(() => undefined)();
  expect(register).not.toHaveBeenCalled();
});
it("initial controller claim never reloads; explicit update applies once", async () => {
  const reload = vi.fn(),
    postMessage = vi.fn();
  const worker = new EventTarget();
  const registration = Object.assign(new EventTarget(), {
    waiting: { postMessage },
    installing: worker,
  });
  const sw = Object.assign(new EventTarget(), {
    controller: {},
    register: vi.fn().mockResolvedValue(registration),
  });
  vi.stubGlobal("window", { isSecureContext: true, location: { reload } });
  vi.stubGlobal("navigator", { serviceWorker: sw });
  let apply: () => void = () => {
    throw Error("missing update");
  };
  const stop = registerShell((v) => {
    apply = v.apply;
  });
  await Promise.resolve();
  sw.dispatchEvent(new Event("controllerchange"));
  expect(reload).not.toHaveBeenCalled();
  apply();
  expect(postMessage).toHaveBeenCalledWith({ type: "ACTIVATE" });
  sw.dispatchEvent(new Event("controllerchange"));
  sw.dispatchEvent(new Event("controllerchange"));
  expect(reload).toHaveBeenCalledOnce();
  stop();
});
