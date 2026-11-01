import { expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { demoSnapshot } from "../testing/demo";
import { Results } from "./results";
it("keeps existing results visible when a snapshot shrinks on page two", async () => {
  const data = demoSnapshot();
  data.results = Array.from({ length: 6 }, (_, i) => ({
    ...data.results[0]!,
    id: `r_${i}`,
  }));
  const view = render(
    <Results snapshot={data} selected={null} onSelect={() => undefined} />,
  );
  await userEvent.click(screen.getByRole("button", { name: "Next" }));
  const next = structuredClone(data);
  next.results = [next.results[0]!];
  view.rerender(
    <Results snapshot={next} selected={null} onSelect={() => undefined} />,
  );
  expect(screen.getByText("r_0", { exact: false })).toBeVisible();
});
import { BudgetMeter } from "../ui/patterns";
it("makes exposure above a zero ceiling explicit", () => {
  const finance = demoSnapshot().finance!;
  finance.ceiling = "0";
  finance.settled = "1";
  render(<BudgetMeter finance={finance} />);
  expect(screen.getByText(/exceeds the approved ceiling/i)).toBeVisible();
});
