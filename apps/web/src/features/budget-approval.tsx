import { useEffect, useRef, useState, type FormEvent } from "react";
import {
  BudgetFailure,
  type BudgetCommand,
  type BudgetProposal,
  type BudgetReceipt,
} from "../api/budget";
import type { Snapshot } from "../api/model";
import { humanSession } from "../auth/session";
import { Button, Checkbox, TextField } from "../ui/controls";
import { ApprovalPreview } from "../ui/patterns";

/** Presentation of one exact action. No optimistic ledger writes or automatic mutation retries. */
export function BudgetApproval({
  snapshot,
  onRefresh,
  active = true,
}: {
  snapshot: Snapshot;
  onRefresh: () => void;
  active?: boolean;
}) {
  const finance = snapshot.finance!;
  const [total, setTotal] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [proposal, setProposal] = useState<BudgetProposal | null>(null);
  const [receipt, setReceipt] = useState<BudgetReceipt | null>(null);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const [uncertain, setUncertain] = useState(false);
  const [remaining, setRemaining] = useState(0);
  const [emailKind, setEmailKind] = useState<"otp" | "magic_link">(
    "magic_link",
  );
  const inFlight = useRef(false);
  const deadline = useRef(0);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  useEffect(() => {
    if (
      !receipt ||
      !("proofExpiresAt" in receipt) ||
      receipt.proofExpiresAt === null
    )
      return;
    deadline.current =
      performance.now() +
      Math.max(0, receipt.proofExpiresAt - receipt.serverNow) * 1000;
    const update = () =>
      setRemaining(
        Math.max(0, Math.ceil((deadline.current - performance.now()) / 1000)),
      );
    update();
    const timer = setInterval(update, 1000);
    return () => clearInterval(timer);
  }, [receipt]);
  const allowed =
    active &&
    !__DEMO__ &&
    snapshot.provenance === "controller" &&
    snapshot.role === "owner" &&
    snapshot.capabilities.budgetIncrease === true;
  const stale =
    proposal !== null &&
    (proposal.expectedVersion !== finance.version ||
      proposal.previous !== finance.ceiling ||
      proposal.currency !== finance.currency);
  const valid =
    /^(0|[1-9][0-9]{0,18})$/.test(total) &&
    BigInt(total) > BigInt(finance.ceiling) &&
    BigInt(total) <= 9223372036854775807n;
  const proof = receipt?.state === "pending" && receipt.proofExpiresAt !== null;
  const run = async (command: BudgetCommand | { status: string }) => {
    if (inFlight.current || !allowed) return;
    inFlight.current = true;
    setBusy(true);
    setMessage("");
    try {
      const next = await humanSession.budget(snapshot.project.id, command);
      if (!mounted.current) return;
      setReceipt(next);
      setUncertain(false);
      if (next.state === "committed") {
        setMessage(
          "Ceiling increase committed. Refreshing the authoritative ledger; no worker was launched.",
        );
        onRefresh();
      }
    } catch (error) {
      if (!mounted.current) return;
      if (error instanceof BudgetFailure && error.status < 500) {
        setMessage(
          error.status === 422
            ? "Fresh verification was denied. Check the current password and any required authenticator code."
            : "The operation was denied or its versions changed. Refresh the ledger before preparing a new proposal.",
        );
      } else {
        setUncertain(true);
        setMessage(
          "The response was lost or could not be verified. The effect is unconfirmed. Inspect its recorded status; do not repeat the approval.",
        );
      }
    } finally {
      inFlight.current = false;
      if (mounted.current) setBusy(false);
    }
  };
  const submitProof = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const form = event.currentTarget;
    const data = new FormData(form);
    form.reset();
    if (
      !proposal ||
      !receipt ||
      receipt.state !== "pending" ||
      stale ||
      uncertain
    )
      return;
    void run({
      stage: "password",
      action: receipt.action,
      proposal,
      password: String(data.get("password") ?? ""),
      totp: receipt.totp ? String(data.get("totp") ?? "") : null,
    });
  };
  return (
    <div className="rv-section-stack" data-ui="budget-approval">
      {!active && (
        <p role="status">
          Revalidating current project authority. Approval commands are paused.
        </p>
      )}
      <p className="rv-note">
        This increases the total approved ceiling only. Existing holds and
        unknown external liabilities remain. Dispatch and admission require
        separate controller authorization.
      </p>
      {!proposal ? (
        <>
          <TextField
            label="New total ceiling · integer minor units"
            inputMode="numeric"
            value={total}
            maxLength={19}
            onChange={(e) => {
              setTotal(e.target.value);
              setConfirmed(false);
            }}
            error={
              total && !valid
                ? "Enter an integer total greater than the current ceiling, within signed 64-bit bounds."
                : undefined
            }
          />
          <ApprovalPreview
            finance={finance}
            actor={humanSession.getSnapshot().session?.user}
            project={snapshot.project.id}
            total={total}
          />
          <Checkbox
            label="I confirm this exact project, currency, new total and budget revision."
            checked={confirmed}
            onChange={(e) => setConfirmed(e.target.checked)}
            disabled={!valid || !allowed}
          />
          <Button
            variant="primary"
            loading={busy}
            disabled={!allowed || !valid || !confirmed || uncertain}
            onClick={() => {
              const exact: BudgetProposal = {
                expectedVersion: finance.version,
                currency: finance.currency,
                previous: finance.ceiling,
                total,
                confirm: true,
              };
              setProposal(exact);
              void run({ stage: "prepare", proposal: exact });
            }}
          >
            Prepare exact approval
          </Button>
        </>
      ) : (
        <>
          <ApprovalPreview
            finance={{
              ...finance,
              version: proposal.expectedVersion,
              ceiling: proposal.previous,
            }}
            actor={humanSession.getSnapshot().session?.user}
            project={snapshot.project.id}
            total={proposal.total}
          />
          {stale && receipt?.state !== "committed" && (
            <p role="alert">
              The financial revision changed. This preview cannot authorize a
              different ceiling. Close and refresh before creating a fresh
              proposal.
            </p>
          )}
          {receipt?.state === "pending" && !proof && !stale && !uncertain && (
            <>
              {receipt.password ? (
                <form className="rv-section-stack" onSubmit={submitProof}>
                  <TextField
                    label="Current password · fresh Owner verification"
                    name="password"
                    type="password"
                    autoComplete="current-password"
                    required
                    maxLength={1024}
                  />
                  {receipt.totp && (
                    <TextField
                      label="Fresh authenticator code"
                      name="totp"
                      inputMode="numeric"
                      autoComplete="one-time-code"
                      pattern="[0-9]{6}"
                      required
                      maxLength={6}
                    />
                  )}
                  <Button
                    type="submit"
                    loading={busy}
                    disabled={!allowed}
                    variant="primary"
                  >
                    Verify this action
                  </Button>
                </form>
              ) : receipt.email ? (
                <div className="rv-section-stack">
                  <p>
                    Your email-only account requires a newly delivered
                    action-specific challenge.
                  </p>
                  <Button
                    loading={busy}
                    onClick={() => {
                      setEmailKind("magic_link");
                      void run({
                        stage: "email",
                        proposal,
                        action: receipt.action,
                        kind: "magic_link",
                      });
                    }}
                  >
                    Send action confirmation
                  </Button>
                  <Button
                    loading={busy}
                    variant="ghost"
                    onClick={() => {
                      setEmailKind("otp");
                      void run({
                        stage: "email",
                        proposal,
                        action: receipt.action,
                        kind: "otp",
                      });
                    }}
                  >
                    Use email code instead
                  </Button>
                </div>
              ) : (
                <p role="status">
                  No approved fresh verification method is operationally
                  available. No approval has been granted.
                </p>
              )}
            </>
          )}
          {receipt?.state === "email_pending" && !stale && !uncertain && (
            <form
              className="rv-section-stack"
              onSubmit={(e) => {
                e.preventDefault();
                const form = e.currentTarget,
                  data = new FormData(form);
                form.reset();
                void run({
                  stage: "email_confirm",
                  proposal,
                  action: receipt.action,
                  challenge: receipt.challenge,
                  kind: emailKind,
                  secret: String(data.get("secret") ?? ""),
                  totp: String(data.get("totp") ?? "") || null,
                });
              }}
            >
              <TextField
                label={
                  emailKind === "otp"
                    ? "Action email code"
                    : "Action confirmation token"
                }
                name="secret"
                autoComplete="off"
                type="password"
                required
                maxLength={128}
              />
              <TextField
                label="Authenticator code, if already enrolled"
                name="totp"
                inputMode="numeric"
                autoComplete="one-time-code"
                maxLength={6}
              />
              <Button type="submit" loading={busy} disabled={!allowed}>
                Confirm delivered proof
              </Button>
            </form>
          )}
          {proof && !stale && (
            <>
              <p role="status">
                Fresh action proof:{" "}
                {remaining > 0
                  ? `${remaining} seconds remaining`
                  : "expired; prepare a new action"}
                . Session renewal cannot extend this deadline.
              </p>
              <Button
                variant="primary"
                loading={busy}
                disabled={remaining === 0 || uncertain || !allowed}
                onClick={() => {
                  if (proposal && receipt)
                    void run({
                      stage: "execute",
                      proposal,
                      action: receipt.action,
                    });
                }}
              >
                Commit exact ceiling increase
              </Button>
            </>
          )}
          {receipt && ["held", "revoked"].includes(receipt.state) && (
            <p role="alert">
              This action is {receipt.state}. It cannot authorize an increase.
            </p>
          )}
          {uncertain && receipt && (
            <Button
              loading={busy}
              disabled={!allowed}
              onClick={() => void run({ status: receipt.action })}
            >
              Inspect recorded approval status
            </Button>
          )}
          {!receipt && !busy && (
            <Button
              disabled={!allowed}
              onClick={() => void run({ stage: "prepare", proposal })}
            >
              Recover prepared proposal
            </Button>
          )}
        </>
      )}
      {message && <p role={uncertain ? "alert" : "status"}>{message}</p>}
      {!allowed && (
        <p className="rv-note">
          This is a read-only preview. A current authenticated Owner and the
          trusted browser command adapter are required.
        </p>
      )}
    </div>
  );
}
