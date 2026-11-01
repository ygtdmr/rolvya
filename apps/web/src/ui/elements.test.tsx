import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Button, Tabs, TextField, Switch } from "./controls";
import { Dialog, Menu, Tooltip } from "./overlays";
describe("shared Elements", () => {
  it("names validation, read-only, loading and disabled states", async () => {
    const action = vi.fn();
    render(
      <>
        <TextField label="Amount" error="Use minor units" readOnly value="20" />
        <Button loading onClick={action}>
          Save
        </Button>
        <Switch label="Motion" disabled />
      </>,
    );
    expect(screen.getByLabelText("Amount")).toHaveAccessibleDescription(
      "Use minor units",
    );
    expect(screen.getByLabelText("Amount")).toHaveAttribute("readonly");
    await userEvent.click(screen.getByRole("button"));
    expect(action).not.toHaveBeenCalled();
    expect(screen.getByRole("switch")).toBeDisabled();
  });
  it("describes the actual focusable tooltip trigger", async () => {
    render(
      <Tooltip label="Plan is frozen">
        <Button>Plan version</Button>
      </Tooltip>,
    );
    await userEvent.tab();
    expect(screen.getByRole("button")).toHaveAccessibleDescription(
      "Plan is frozen",
    );
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
  });
  it("supports arrow and boundary navigation in tabs", async () => {
    const change = vi.fn();
    render(
      <Tabs
        panelId="test-view"
        label="Views"
        value="tree"
        options={[
          { value: "tree", label: "Tree" },
          { value: "graph", label: "Graph" },
        ]}
        onChange={change}
      />,
    );
    await userEvent.tab();
    await userEvent.keyboard("{End}");
    expect(change).toHaveBeenCalledWith("graph");
    expect(screen.getByRole("tab", { name: "Graph" })).toHaveFocus();
  });
  it("traps a dialog and restores focus when closed", async () => {
    const close = vi.fn();
    const view = render(
      <>
        <Button>Inspect</Button>
        <Dialog open={false} onClose={close} title="Details">
          <Button>Last</Button>
        </Dialog>
      </>,
    );
    screen.getByText("Inspect").focus();
    view.rerender(
      <>
        <Button>Inspect</Button>
        <Dialog open onClose={close} title="Details">
          <Button>Last</Button>
        </Dialog>
      </>,
    );
    expect(screen.getByRole("button", { name: "Close" })).toHaveFocus();
    await userEvent.keyboard("{Shift>}{Tab}{/Shift}");
    expect(screen.getByText("Last")).toHaveFocus();
    await userEvent.keyboard("{Escape}");
    expect(close).toHaveBeenCalled();
    view.rerender(
      <>
        <Button>Inspect</Button>
        <Dialog open={false} onClose={close} title="Details">
          <Button>Last</Button>
        </Dialog>
      </>,
    );
    expect(screen.getByText("Inspect")).toHaveFocus();
  });
  it("skips disabled menu items and returns focus", async () => {
    const select = vi.fn();
    render(
      <Menu
        label="Actions"
        items={[
          { label: "Unavailable", disabled: true, onSelect: select },
          { label: "Inspect", onSelect: select },
        ]}
      />,
    );
    await userEvent.click(screen.getByRole("button"));
    expect(screen.getByRole("menuitem", { name: "Inspect" })).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    expect(select).toHaveBeenCalledOnce();
    expect(screen.getByRole("button")).toHaveFocus();
  });
});
it("reverses horizontal tabs under RTL and links its panel", async () => {
  const change = vi.fn();
  render(
    <div dir="rtl">
      <Tabs
        panelId="rtl-view"
        label="RTL views"
        value="a"
        options={[
          { value: "a", label: "First" },
          { value: "b", label: "Second" },
          { value: "c", label: "Third" },
        ]}
        onChange={change}
      />
    </div>,
  );
  await userEvent.tab();
  await userEvent.keyboard("{ArrowRight}");
  expect(change).toHaveBeenCalledWith("c");
  expect(screen.getByRole("tab", { name: "First" })).toHaveAttribute(
    "aria-controls",
  );
});
