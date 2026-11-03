import {
  useEffect,
  useState,
  useSyncExternalStore,
  type FormEvent,
  type ReactNode,
} from "react";
import { Button, Checkbox, Select, TextField } from "../ui/controls";
import { EmptyState } from "../ui/data";
import type { Preferences } from "../app/preferences";
import { humanSession } from "./session";
export function SessionGate({
  children,
  preferences,
  setPreferences,
}: {
  children: ReactNode;
  preferences: Preferences;
  setPreferences: (next: Preferences) => void;
}) {
  const state = useSyncExternalStore(
    humanSession.subscribe,
    humanSession.getSnapshot,
  );
  const [show, setShow] = useState(false),
    [alternative, setAlternative] = useState(false),
    [emailKind, setEmailKind] = useState("magic_link"),
    [recovery, setRecovery] = useState(false);
  useEffect(() => {
    void humanSession.start();
    const confirmLink = () => {
      if (location.hash.startsWith("#email-confirm="))
        void humanSession.start();
    };
    window.addEventListener("hashchange", confirmLink);
    return () => {
      window.removeEventListener("hashchange", confirmLink);
      humanSession.dispose();
    };
  }, []);
  useEffect(() => {
    if (state.phase !== "authenticated") document.title = "Sign in · Rolvya";
    else document.title = "Rolvya · Connected Control";
  }, [state.phase]);
  if (state.phase === "authenticated") return children;
  const busy = state.phase === "authenticating" || state.phase === "checking";
  const sessionOperation =
    state.phase === "renewing" || state.phase === "signing_out";
  const factor = state.stage === "factor";
  const pending = state.stage === "email";
  const link = state.stage === "link";
  const unavailable = state.phase === "unavailable";
  const emailFlow = alternative || (!state.password && state.email);
  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const form = event.currentTarget;
    const values = new FormData(form);
    for (const name of ["password", "code"]) {
      const field = form.elements.namedItem(name);
      if (field instanceof HTMLInputElement) field.value = "";
    }
    try {
      if (factor)
        await humanSession.factor(
          recovery ? "recovery" : "totp",
          String(values.get("code") ?? ""),
        );
      else if (pending)
        await humanSession.confirmEmail(
          emailKind as "magic_link" | "otp",
          String(values.get("code") ?? ""),
        );
      else if (emailFlow)
        await humanSession.email(
          String(values.get("email") ?? ""),
          emailKind as "magic_link" | "otp",
        );
      else
        await humanSession.password(
          String(values.get("email") ?? ""),
          String(values.get("password") ?? ""),
        );
    } finally {
      for (const name of ["password", "code"]) {
        const field = form.elements.namedItem(name);
        if (field instanceof HTMLInputElement) field.value = "";
      }
    }
  };
  return (
    <div className="rv-auth" data-ui="human-auth">
      <header className="rv-auth-header">
        <a href="/" className="rv-auth-brand" aria-label="Rolvya sign in">
          Rolvya
        </a>
        <Select
          label="Theme"
          value={preferences.theme}
          options={[
            { value: "system", label: "System" },
            { value: "light", label: "Light" },
            { value: "dark", label: "Dark" },
          ]}
          onChange={(theme) =>
            setPreferences({
              ...preferences,
              theme: theme as Preferences["theme"],
            })
          }
        />
      </header>
      <main className="rv-auth-main" aria-labelledby="rv-auth-title">
        <div className="rv-auth-intro">
          <p className="rv-eyebrow">CONNECTED CONTROL</p>
          <h1 id="rv-auth-title">
            {factor
              ? "Verify your sign-in"
              : link
                ? "Confirm your sign-in"
                : pending
                  ? "Continue with email"
                  : "Sign in to Rolvya"}
          </h1>
          <p>
            {factor
              ? "Use your enrolled authenticator or a one-use recovery code."
              : "Your workspace. Your authority."}
          </p>
        </div>
        {state.message && (
          <p className="rv-auth-message" role="alert">
            {state.message}
          </p>
        )}
        {sessionOperation ? (
          <p role="status">
            {state.phase === "signing_out"
              ? "Revoking your server session…"
              : "Refreshing your session…"}
          </p>
        ) : state.phase === "checking" ? (
          <p role="status">Checking secure connection…</p>
        ) : unavailable ? (
          <>
            <EmptyState title="Secure sign-in unavailable">
              {state.message}
            </EmptyState>
            <Button onClick={() => void humanSession.start()}>
              Check connection
            </Button>
          </>
        ) : !state.password &&
          !state.email &&
          !factor &&
          !pending &&
          !link &&
          !busy ? (
          <>
            <EmptyState title="Sign-in methods unavailable">
              Check the secure service before starting another sign-in.
            </EmptyState>
            <Button onClick={() => void humanSession.beginAgain()}>
              Check sign-in availability
            </Button>
          </>
        ) : link ? (
          <>
            <p className="rv-help">
              Opening an email link does not sign you in. Confirm only the
              request you started in this browser.
            </p>
            <Button
              disabled={!state.email || busy}
              loading={busy}
              onClick={() => void humanSession.confirmLink()}
            >
              Confirm sign-in
            </Button>
            <Button
              variant="ghost"
              onClick={() => void humanSession.beginAgain()}
            >
              Begin sign-in again
            </Button>
          </>
        ) : (
          <form
            method="post"
            action="/api/control/v1/auth/password"
            onSubmit={(event) => void submit(event)}
            aria-busy={busy}
            className="rv-auth-form"
          >
            {!factor && !pending && (
              <TextField
                label="Email address"
                name="email"
                type="email"
                autoComplete="username"
                required
                maxLength={254}
                disabled={busy}
              />
            )}
            {factor || pending ? (
              <TextField
                key={recovery ? "recovery" : "code"}
                label={
                  factor
                    ? recovery
                      ? "Recovery code"
                      : "Authenticator code"
                    : emailKind === "otp"
                      ? "Email code"
                      : "Email confirmation code"
                }
                name="code"
                type="text"
                autoComplete="one-time-code"
                inputMode={
                  recovery || (emailKind === "magic_link" && !factor)
                    ? "text"
                    : "numeric"
                }
                minLength={
                  factor ? (recovery ? 32 : 6) : emailKind === "otp" ? 8 : 64
                }
                maxLength={
                  factor ? (recovery ? 32 : 6) : emailKind === "otp" ? 8 : 64
                }
                required
                disabled={busy}
              />
            ) : emailFlow ? (
              <Select
                label="Email method"
                value={emailKind}
                onChange={setEmailKind}
                options={[
                  { value: "magic_link", label: "Magic Link" },
                  { value: "otp", label: "One-time email code" },
                ]}
                disabled={busy}
              />
            ) : (
              <>
                <TextField
                  label="Password"
                  name="password"
                  type={show ? "text" : "password"}
                  autoComplete="current-password"
                  required
                  minLength={12}
                  maxLength={1024}
                  disabled={busy}
                />
                <Checkbox
                  label="Show password"
                  checked={show}
                  onChange={(event) => setShow(event.target.checked)}
                  disabled={busy}
                />
              </>
            )}
            <Button
              type="submit"
              loading={busy}
              disabled={!factor && !pending && !emailFlow && !state.password}
            >
              {factor || pending
                ? "Verify and sign in"
                : emailFlow
                  ? "Continue with email"
                  : "Sign in"}
            </Button>
            {factor && (
              <Button
                variant="ghost"
                disabled={busy}
                onClick={() => setRecovery(!recovery)}
              >
                {recovery ? "Use authenticator instead" : "Use a recovery code"}
              </Button>
            )}
            {state.email && state.password && !factor && !pending && (
              <Button
                variant="ghost"
                disabled={busy}
                onClick={() => setAlternative(!alternative)}
              >
                {alternative ? "Use password instead" : "Other sign-in methods"}
              </Button>
            )}
            {(factor ||
              pending ||
              state.phase === "expired" ||
              state.phase === "revoked") && (
              <Button
                variant="ghost"
                disabled={busy}
                onClick={() => void humanSession.beginAgain()}
              >
                Begin sign-in again
              </Button>
            )}
          </form>
        )}
        <details className="rv-auth-recovery">
          <summary>Need help signing in?</summary>
          <p>
            Self-service password reset is unavailable. Contact the instance
            administrator.{" "}
            {state.supportUrl ? (
              <a href={state.supportUrl} rel="noreferrer">
                Contact instance support
              </a>
            ) : (
              "No support contact is configured."
            )}
          </p>
          <p>
            Email login is available only when operational delivery is
            independently verified. This screen cannot create an account or
            grant project access.
          </p>
        </details>
        <footer className="rv-auth-footer">One Plan. Many Agents.</footer>
      </main>
    </div>
  );
}
