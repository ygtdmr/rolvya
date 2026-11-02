import { expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { demoSnapshot } from "../testing/demo";
import { humanSession } from "../auth/session";
import { BudgetApproval } from "./budget-approval";
it("demo preview cannot authorize money even with an Owner display flag", async () => {
  const data = demoSnapshot();
  data.role = "owner";
  data.capabilities.budgetIncrease = true;
  const command = vi.spyOn(humanSession, "budget");
  render(<BudgetApproval snapshot={data} onRefresh={vi.fn()} />);
  await userEvent.type(
    screen.getByLabelText("New total ceiling · integer minor units"),
    "999999",
  );
  expect(screen.getByRole("checkbox")).toBeDisabled();
  expect(
    screen.getByRole("button", { name: "Prepare exact approval" }),
  ).toBeDisabled();
  expect(command).not.toHaveBeenCalled();
  command.mockRestore();
});
