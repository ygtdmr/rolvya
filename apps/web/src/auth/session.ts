export type AuthPhase =
  | "checking"
  | "unauthenticated"
  | "authenticating"
  | "renewing"
  | "signing_out"
  | "factor_required"
  | "email_pending"
  | "email_confirmation"
  | "authenticated"
  | "expired"
  | "revoked"
  | "unavailable";
export interface HumanSession {
  user: string;
  session: string;
  generation: number;
  assurance: "primary" | "totp" | "recovery";
  absoluteExpires: number;
  lastHumanAt: number;
}
export interface AuthState {
  phase: AuthPhase;
  stage: "primary" | "factor" | "email" | "link";
  password: boolean;
  email: boolean;
  message: string;
  session: HumanSession | null;
  challenge: string | null;
}
const initial: AuthState = {
  phase: "checking",
  stage: "primary",
  password: false,
  email: false,
  message: "",
  session: null,
  challenge: null,
};
const hex = (value: unknown, length: number): value is string =>
  typeof value === "string" &&
  value.length === length &&
  /^[a-f0-9]+$/.test(value);
class AuthFailure extends Error {
  constructor(readonly status: number) {
    super("Authentication request failed");
  }
}
type Endpoint =
  | "context"
  | "session"
  | "password"
  | "factor"
  | "email"
  | "email/confirm"
  | "renew"
  | "activity"
  | "interaction"
  | "logout"
  | "logout-all";
/** Presentation and CSRF memory only. Human session handles remain HttpOnly cookies. */
export class BrowserSession {
  #state: AuthState = initial;
  #csrf: string | undefined;
  #preauth: string | undefined;
  #link: { challenge: string; secret: string } | undefined;
  #listeners = new Set<() => void>();
  #abort = new AbortController();
  #epoch = 0;
  #channel: BroadcastChannel | undefined;
  #foregroundListening = false;
  #activityBusy = false;
  #nextActivity = 0;
  #tokenDeadline = 0;
  // A trusted browser gesture is application activity, not independent physical
  // presence attestation. A thief with a current cookie and CSRF remains a risk.
  #foreground = (event: Event) => {
    if (
      !event.isTrusted ||
      document.visibilityState !== "visible" ||
      !document.hasFocus() ||
      !navigator.onLine
    )
      return;
    void this.#recordInteraction();
  };
  async #recordInteraction() {
    if (
      this.#activityBusy ||
      this.#state.phase !== "authenticated" ||
      !this.#csrf ||
      performance.now() < this.#nextActivity
    )
      return;
    const epoch = this.#epoch;
    const csrf = this.#csrf;
    this.#activityBusy = true;
    this.#nextActivity = performance.now() + 30000;
    try {
      const value = await this.#request("activity", {}, csrf);
      if (epoch !== this.#epoch || this.#state.phase !== "authenticated")
        return;
      if (!hex(value.intent, 64) || !Number.isSafeInteger(value.issued))
        throw new AuthFailure(503);
      const recorded = await this.#request(
        "interaction",
        { intent: value.intent, issued: value.issued },
        csrf,
      );
      if (epoch !== this.#epoch) return;
      if (
        recorded.state !== "interaction_recorded" ||
        !Number.isSafeInteger(recorded.lastHumanAt)
      )
        throw new AuthFailure(503);
      this.#nextActivity = performance.now() + 300000;
      if (this.#state.session)
        this.#set({
          session: {
            ...this.#state.session,
            lastHumanAt: Number(recorded.lastHumanAt),
          },
        });
      // Renew only on confirmed foreground use near expiry. No polling timer,
      // SSE event, service worker or lost-response retry can rotate the cookie.
      if (this.#tokenDeadline - performance.now() < 600000) await this.renew();
    } catch (error) {
      if (
        epoch === this.#epoch &&
        error instanceof AuthFailure &&
        [401, 403].includes(error.status)
      )
        await this.invalidate();
    } finally {
      this.#activityBusy = false;
    }
  }
  getSnapshot = () => this.#state;
  subscribe = (listener: () => void) => {
    this.#listeners.add(listener);
    return () => {
      this.#listeners.delete(listener);
    };
  };
  #set(next: Partial<AuthState>) {
    this.#state = { ...this.#state, ...next };
    this.#listeners.forEach((f) => f());
  }
  async #request(
    endpoint: Endpoint,
    body?: Record<string, unknown>,
    csrf?: string,
  ): Promise<Record<string, unknown>> {
    if (location.protocol !== "https:") throw new AuthFailure(503);
    const timeout = new AbortController();
    const abort = () => timeout.abort();
    this.#abort.signal.addEventListener("abort", abort, { once: true });
    const timer = setTimeout(abort, 10000);
    try {
      const response = await fetch(`/api/control/v1/auth/${endpoint}`, {
        method: body ? "POST" : "GET",
        credentials: "same-origin",
        cache: "no-store",
        redirect: "error",
        signal: timeout.signal,
        headers: {
          Accept: "application/json",
          ...(body ? { "Content-Type": "application/json" } : {}),
          ...(csrf ? { "X-Rolvya-CSRF": csrf } : {}),
        },
        ...(body ? { body: JSON.stringify(body) } : {}),
      });
      if (!response.ok) throw new AuthFailure(response.status);
      if (
        !response.headers.get("content-type")?.startsWith("application/json") ||
        !response.body
      )
        throw new AuthFailure(503);
      const reader = response.body.getReader();
      const chunks: Uint8Array[] = [];
      let size = 0;
      try {
        for (;;) {
          const next = await reader.read();
          if (next.done) break;
          size += next.value.length;
          if (size > 16384) throw new AuthFailure(503);
          chunks.push(next.value);
        }
      } finally {
        await reader.cancel().catch(() => undefined);
        reader.releaseLock();
      }
      const bytes = new Uint8Array(size);
      let offset = 0;
      for (const chunk of chunks) {
        bytes.set(chunk, offset);
        offset += chunk.length;
      }
      const value: unknown = JSON.parse(
        new TextDecoder("utf-8", { fatal: true }).decode(bytes),
      );
      if (
        typeof value !== "object" ||
        value === null ||
        Array.isArray(value) ||
        !("schema" in value) ||
        value.schema !== 1
      )
        throw new AuthFailure(503);
      return value as Record<string, unknown>;
    } finally {
      clearTimeout(timer);
      this.#abort.signal.removeEventListener("abort", abort);
    }
  }
  async #inspect(epoch: number) {
    const value = await this.#request("session");
    if (
      value.state !== "authenticated" ||
      !hex(value.user, 32) ||
      !hex(value.session, 32) ||
      !hex(value.csrf, 64) ||
      !Number.isSafeInteger(value.generation) ||
      Number(value.generation) < 1 ||
      !["primary", "totp", "recovery"].includes(String(value.assurance)) ||
      !Number.isSafeInteger(value.absoluteExpires) ||
      !Number.isSafeInteger(value.lastHumanAt) ||
      !Number.isSafeInteger(value.tokenExpiresAt) ||
      !Number.isSafeInteger(value.serverNow) ||
      Number(value.tokenExpiresAt) <= Number(value.serverNow)
    )
      throw new AuthFailure(503);
    if (epoch !== this.#epoch) return;
    this.#csrf = value.csrf;
    this.#tokenDeadline =
      performance.now() +
      (Number(value.tokenExpiresAt) - Number(value.serverNow)) * 1000;
    this.#link = undefined;
    this.#set({
      phase: "authenticated",
      stage: "primary",
      session: {
        user: value.user,
        session: value.session,
        generation: Number(value.generation),
        assurance: value.assurance as HumanSession["assurance"],
        absoluteExpires: Number(value.absoluteExpires),
        lastHumanAt: Number(value.lastHumanAt),
      },
      message: "",
      challenge: null,
    });
  }
  async #initialize(
    epoch: number,
    phase: AuthPhase = "unauthenticated",
    message = "",
  ) {
    const value = await this.#request("context");
    if (
      !hex(value.csrf, 64) ||
      typeof value.password !== "boolean" ||
      typeof value.email !== "boolean"
    )
      throw new AuthFailure(503);
    if (epoch !== this.#epoch) return;
    this.#preauth = value.csrf;
    this.#set({
      phase: this.#link ? "email_confirmation" : phase,
      stage: this.#link ? "link" : "primary",
      password: value.password,
      email: value.email,
      message:
        this.#link && !value.email
          ? "Email sign-in is unavailable. No delivery was confirmed."
          : message,
      session: null,
      challenge: this.#link?.challenge ?? null,
    });
  }
  start = async () => {
    const epoch = ++this.#epoch;
    this.#set({ phase: "checking", session: null });
    if (!this.#foregroundListening) {
      document.addEventListener("pointerdown", this.#foreground, {
        passive: true,
      });
      document.addEventListener("keydown", this.#foreground);
      this.#foregroundListening = true;
    }
    // A link is a one-use challenge, never a session cookie. Remove it from the
    // visible URL immediately; an explicit POST and original browser cookie are required.
    const fragment = location.hash;
    if (fragment.startsWith("#email-confirm=")) {
      history.replaceState(null, "", location.pathname + location.search);
      const [challenge, secret, ...extra] = fragment.slice(15).split(".");
      if (hex(challenge, 32) && hex(secret, 64) && !extra.length)
        this.#link = { challenge, secret };
    }
    if (!this.#channel && typeof BroadcastChannel !== "undefined") {
      this.#channel = new BroadcastChannel("rolvya-human-session-v1");
      this.#channel.addEventListener(
        "message",
        (event: MessageEvent<unknown>) => {
          if (
            event.origin !== location.origin ||
            !event.data ||
            typeof event.data !== "object" ||
            !("type" in event.data) ||
            !["signed_out", "rotated"].includes(String(event.data.type))
          )
            return;
          if (event.data.type === "rotated") void this.start();
          else void this.invalidate("expired", "Signed out in another tab.");
        },
      );
    }
    try {
      await this.#inspect(epoch);
    } catch (error) {
      if (epoch !== this.#epoch) return;
      if (error instanceof AuthFailure && [401, 403].includes(error.status)) {
        try {
          await this.#initialize(epoch);
        } catch {
          if (epoch === this.#epoch)
            this.#set({
              phase: "unavailable",
              message: "Secure sign-in is temporarily unavailable.",
              session: null,
            });
        }
      } else
        this.#set({
          phase: "unavailable",
          message:
            location.protocol !== "https:"
              ? "A trusted HTTPS connection is required to sign in."
              : "Unable to reach secure sign-in. No authentication was confirmed.",
          session: null,
        });
    }
  };
  async #login(
    endpoint: "password" | "factor" | "email" | "email/confirm",
    body: Record<string, unknown>,
  ) {
    if (this.#state.phase === "authenticating" || !this.#preauth) return;
    const previous = this.#state.phase;
    const epoch = ++this.#epoch;
    this.#set({ phase: "authenticating", message: "", session: null });
    try {
      const value = await this.#request(endpoint, body, this.#preauth);
      if (epoch !== this.#epoch) return;
      if (value.state === "authenticated") {
        this.#link = undefined;
        await this.#inspect(epoch);
      } else if (value.state === "factor_required")
        this.#set({
          phase: "factor_required",
          stage: "factor",
          message: "Complete your enrolled second factor.",
        });
      else if (value.state === "email_pending" && hex(value.challenge, 32))
        this.#set({
          phase: "email_pending",
          stage: "email",
          challenge: value.challenge,
          message:
            "If this address is eligible, continue using its email message. Delivery is not confirmed by this receipt.",
        });
      else throw new AuthFailure(503);
    } catch (error) {
      if (epoch !== this.#epoch) return;
      const limited = error instanceof AuthFailure && error.status === 429;
      const denied =
        error instanceof AuthFailure && [400, 401, 403].includes(error.status);
      this.#set({
        phase: previous,
        session: null,
        message: limited
          ? "Too many attempts. Wait before trying again."
          : denied
            ? "Unable to verify these credentials. Try again or begin sign-in again."
            : "Authentication was not confirmed. Check your connection before trying again.",
      });
    }
  }
  password = (email: string, password: string) =>
    this.#login("password", { email, password });
  factor = (kind: "totp" | "recovery", code: string) =>
    this.#login("factor", { kind, code });
  email = (email: string, kind: "magic_link" | "otp") =>
    this.#login("email", { email, kind });
  confirmEmail = (kind: "magic_link" | "otp", secret: string) =>
    this.#login("email/confirm", {
      challenge: this.#state.challenge,
      kind,
      secret,
      confirm: true,
    });
  confirmLink = () =>
    this.#link
      ? this.#login("email/confirm", {
          ...this.#link,
          kind: "magic_link",
          confirm: true,
        })
      : Promise.resolve();
  beginAgain = async () => {
    this.#link = undefined;
    const epoch = ++this.#epoch;
    this.#set({
      phase: "checking",
      session: null,
      message: "",
      challenge: null,
    });
    try {
      await this.#initialize(epoch);
    } catch {
      if (epoch === this.#epoch)
        this.#set({
          phase: "unavailable",
          message: "Secure sign-in is temporarily unavailable.",
        });
    }
  };
  invalidate = async (
    phase: "expired" | "revoked" = "expired",
    message = "Your session is no longer available. Sign in again.",
  ) => {
    this.#abort.abort();
    this.#abort = new AbortController();
    this.#csrf = undefined;
    const epoch = ++this.#epoch;
    this.#set({ phase, session: null, message });
    try {
      await this.#initialize(epoch, phase, message);
    } catch {
      /* Keep the denied state; never restore private data. */
    }
  };
  renew = async () => {
    if (this.#state.phase !== "authenticated" || !this.#csrf) return;
    const epoch = ++this.#epoch;
    const csrf = this.#csrf;
    this.#set({ phase: "renewing", session: null });
    try {
      const value = await this.#request("renew", {}, csrf);
      if (value.state !== "authenticated") throw new AuthFailure(503);
      await this.#inspect(epoch);
      if (epoch === this.#epoch)
        this.#channel?.postMessage({ type: "rotated" });
    } catch {
      if (epoch === this.#epoch)
        await this.invalidate(
          "expired",
          "Session renewal was not confirmed. Sign in again.",
        );
    }
  };
  logout = async (all = false) => {
    if (this.#state.phase !== "authenticated" || !this.#csrf) return;
    const epoch = ++this.#epoch;
    const csrf = this.#csrf;
    this.#set({ phase: "signing_out", session: null });
    let confirmed = false;
    try {
      const value = await this.#request(
        all ? "logout-all" : "logout",
        {},
        csrf,
      );
      if (value.state !== "signed_out") throw new AuthFailure(503);
      if (epoch !== this.#epoch) return;
      confirmed = true;
      this.#csrf = undefined;
      this.#preauth = undefined;
      this.#channel?.postMessage({ type: "signed_out" });
      await this.#initialize(epoch, "unauthenticated", "Signed out.");
    } catch {
      if (epoch === this.#epoch) {
        this.#csrf = undefined;
        this.#preauth = undefined;
        this.#set({
          phase: confirmed ? "unauthenticated" : "unavailable",
          stage: "primary",
          password: false,
          email: false,
          session: null,
          message: confirmed
            ? "Signed out. Preparing a new sign-in is temporarily unavailable."
            : "Sign-out was not confirmed. Your server session may still be active.",
        });
      }
    }
  };
  dispose = () => {
    this.#epoch++;
    this.#abort.abort();
    this.#abort = new AbortController();
    this.#channel?.close();
    this.#channel = undefined;
    this.#csrf = undefined;
    this.#preauth = undefined;
    document.removeEventListener("pointerdown", this.#foreground);
    document.removeEventListener("keydown", this.#foreground);
    this.#foregroundListening = false;
  };
}
export const humanSession = new BrowserSession();
