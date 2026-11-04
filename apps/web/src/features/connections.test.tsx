import { expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { demoSnapshot } from "../testing/demo";
import { Connections } from "./connections";
it("same-project permission downgrade removes an already-open Owner connection dialog", async () => {
  const data = demoSnapshot();
  data.role = "owner";
  const view = render(<Connections snapshot={data} />);
  await userEvent.click(
    screen.getAllByRole("button", { name: "Inspect authority" })[0]!,
  );
  expect(screen.getByRole("dialog")).toBeInTheDocument();
  view.rerender(
    <Connections
      snapshot={{
        ...data,
        role: "viewer",
        connections: [],
        capabilities: { ...data.capabilities, connections: false },
      }}
    />,
  );
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(
    screen.queryByText(data.connections[0]!.account, { exact: true }),
  ).not.toBeInTheDocument();
});
it("reports inspected and simulated qualification without presenting paid execution authority", () => {
  const data = demoSnapshot();
  data.role = "owner";
  data.qualification = {
    configurationInspected: 1,
    simulated: 9,
    untrusted: 0,
    independentlyQualified: 0,
    paidExecutionAllowed: false,
    blockers: ["effective_iam", "cost_enforcement"],
  };
  render(<Connections snapshot={data} />);
  expect(screen.getByText("Production execution blocked")).toBeInTheDocument();
  expect(screen.getByText("Effective IAM permissions")).toBeInTheDocument();
  expect(
    screen.getByText("Enforceable financial exposure"),
  ).toBeInTheDocument();
});
