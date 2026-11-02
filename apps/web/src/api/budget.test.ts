import { expect, it } from "vitest";
import { parseBudgetReceipt } from "./budget";
it("accepts only exact versioned budget receipts and server proof deadlines", () => {
  const value = {
    schema: 1,
    action: "a".repeat(32),
    state: "pending",
    password: true,
    email: false,
    totp: false,
    serverNow: 10000,
    proofExpiresAt: 10300,
  };
  expect(parseBudgetReceipt(value)).toMatchObject({ proofExpiresAt: 10300 });
  for (const patch of [
    { schema: 2 },
    { action: "other" },
    { email: true },
    { serverNow: "10000" },
    { proofExpiresAt: -1 },
    { state: "committed", financialSequence: 0 },
  ])
    expect(() => parseBudgetReceipt({ ...value, ...patch })).toThrow();
});
