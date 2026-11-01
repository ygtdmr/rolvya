import { expect, it } from "vitest";
import { demoSnapshot } from "../testing/demo";
import { graphNodes } from "./TaskGraph";
it("never invents group containment from A/B/G names", () => {
  const tasks = demoSnapshot().tasks.map((t) => ({ ...t, parent: null }));
  expect(
    graphNodes(tasks, null).find((n) => n.id === "A")?.parentId,
  ).toBeUndefined();
});
it("uses real hierarchy for arbitrary task IDs", () => {
  const tasks = demoSnapshot()
    .tasks.slice(0, 2)
    .map((t) => ({
      ...t,
      id: t.id === "G" ? "group_x" : "leaf_y",
      parent: t.kind === "leaf" ? "group_x" : null,
    }));
  expect(graphNodes(tasks, null).find((n) => n.id === "leaf_y")?.parentId).toBe(
    "group_x",
  );
});
