import { ReadError } from "./client";
export interface BudgetProposal {
  expectedVersion: number;
  currency: string;
  previous: string;
  total: string;
  confirm: true;
}
export type BudgetCommand =
  | { stage: "prepare"; proposal: BudgetProposal }
  | {
      stage: "password";
      proposal: BudgetProposal;
      action: string;
      password: string;
      totp: string | null;
    }
  | {
      stage: "email";
      proposal: BudgetProposal;
      action: string;
      kind: "otp" | "magic_link";
    }
  | {
      stage: "email_confirm";
      proposal: BudgetProposal;
      action: string;
      challenge: string;
      kind: "otp" | "magic_link";
      secret: string;
      totp: string | null;
    }
  | { stage: "execute"; proposal: BudgetProposal; action: string };
export type BudgetReceipt =
  | {
      state: "pending" | "held" | "revoked";
      action: string;
      password: boolean;
      email: boolean;
      totp: boolean;
      proofExpiresAt: number | null;
      serverNow: number;
    }
  | { state: "email_pending"; action: string; challenge: string }
  | { state: "committed"; action: string; financialSequence: number };
export class BudgetFailure extends Error {
  constructor(readonly status: number) {
    super("Budget operation was not confirmed");
  }
}
export function parseBudgetReceipt(
  value: Record<string, unknown>,
): BudgetReceipt {
  const hex = (v: unknown) => typeof v === "string" && /^[a-f0-9]{32}$/.test(v);
  const integer = (v: unknown) => Number.isSafeInteger(v) && Number(v) >= 0;
  if (value.schema !== 1 || !hex(value.action)) throw new ReadError("invalid");
  if (
    value.state === "committed" &&
    integer(value.financialSequence) &&
    Number(value.financialSequence) > 0
  )
    return value as unknown as BudgetReceipt;
  if (value.state === "email_pending" && hex(value.challenge))
    return value as unknown as BudgetReceipt;
  if (
    ["pending", "held", "revoked"].includes(String(value.state)) &&
    ["password", "email", "totp"].every(
      (key) => typeof value[key] === "boolean",
    ) &&
    integer(value.serverNow) &&
    (value.proofExpiresAt === null || integer(value.proofExpiresAt)) &&
    !(value.password && value.email)
  )
    return value as unknown as BudgetReceipt;
  throw new ReadError("invalid");
}
