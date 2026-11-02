import { expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { demoSnapshot } from "../testing/demo";
import { Execution } from "./execution";
it("a paginated attempt window never claims the project has zero active workers", () => {
  const data = demoSnapshot();
  data.tasks[0]!.status = "running";
  const sample = data.attempts[0]!;
  data.attempts = Array.from({ length: 20 }, (_, i) => ({
    ...sample,
    id: `historical_${i}`,
    state: "accepted" as const,
  }));
  render(<Execution snapshot={data} selected={null} onSelect={vi.fn()} />);
  expect(
    screen.getByText("0 running attempts on this page"),
  ).toBeInTheDocument();
});
