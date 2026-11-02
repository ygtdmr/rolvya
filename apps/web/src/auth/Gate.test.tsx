import { render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { SessionGate } from "./Gate";
import { loadPreferences } from "../app/preferences";
vi.mock("./session", () => {
  const state = {
    phase: "unauthenticated",
    stage: "primary",
    password: true,
    email: false,
    message: "",
    session: null,
    challenge: null,
  };
  return {
    humanSession: {
      getSnapshot: () => state,
      subscribe: () => () => undefined,
      start: async () => undefined,
      dispose: () => undefined,
    },
  };
});
it("native form fallback cannot place authentication input in a GET URL", () => {
  render(
    <SessionGate
      preferences={loadPreferences()}
      setPreferences={() => undefined}
    >
      <p>Private panel</p>
    </SessionGate>,
  );
  const password = screen.getByLabelText("Password", { exact: true });
  const form = password.closest("form");
  expect(form?.method).toBe("post");
  expect(form?.getAttribute("action")).toBe("/api/control/v1/auth/password");
  expect(screen.queryByText("Private panel")).not.toBeInTheDocument();
});
