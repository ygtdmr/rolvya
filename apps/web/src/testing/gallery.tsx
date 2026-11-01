import { useState } from "react";
import {
  Button,
  Checkbox,
  IconButton,
  SearchField,
  Select,
  Switch,
  Tabs,
  TextField,
} from "../ui/controls";
import {
  Badge,
  DataTable,
  EmptyState,
  ErrorState,
  Progress,
  Skeleton,
  StatusBadge,
} from "../ui/data";
import { Dialog, Menu, Popover, Tooltip } from "../ui/overlays";
export function Gallery() {
  const [open, setOpen] = useState(false),
    [tab, setTab] = useState("one");
  return (
    <div className="rv-section-stack">
      <p className="rv-note">
        Development-only UI Elements gallery. No operational authority.
      </p>
      <div className="rv-filter-bar">
        <Button>Primary</Button>
        <Button variant="secondary">Secondary</Button>
        <Button disabled>Disabled</Button>
        <Button loading>Loading</Button>
        <IconButton label="Labeled icon" icon="info" />
        <Tooltip label="A meaningful explanation">
          <Button variant="ghost">Tooltip</Button>
        </Tooltip>
      </div>
      <TextField label="Field" hint="Persistent help text" />
      <TextField
        label="Invalid field"
        error="Example error"
        defaultValue="Invalid"
      />
      <TextField label="Read-only field" value="Immutable" readOnly />
      <SearchField label="Search Element" />
      <Select label="Select Element">
        <option>First option</option>
      </Select>
      <Checkbox label="Checkbox Element" />
      <Switch label="Switch Element" />
      <Tabs
        panelId="gallery-view"
        label="Gallery tabs"
        value={tab}
        onChange={setTab}
        options={[
          { value: "one", label: "One" },
          { value: "two", label: "Two" },
        ]}
      />
      <div
        role="tabpanel"
        id="gallery-view"
        aria-labelledby={`gallery-view-tab-${tab}`}
      >
        {tab === "one" ? "First gallery panel" : "Second gallery panel"}
      </div>
      <div className="rv-filter-bar">
        <Menu
          label="Menu"
          items={[
            { label: "Inspect", onSelect: () => setOpen(true) },
            { label: "Unavailable", onSelect: () => undefined, disabled: true },
          ]}
        />
        <Popover label="Popover">
          <p>Shared focus and dismissal behavior.</p>
        </Popover>
        <Button onClick={() => setOpen(true)}>Open dialog</Button>
      </div>
      <Dialog open={open} title="Shared dialog" onClose={() => setOpen(false)}>
        <TextField label="Dialog field" />
        <Button onClick={() => setOpen(false)}>Done</Button>
      </Dialog>
      <div className="rv-inline-facts">
        <StatusBadge status="verified" />
        <StatusBadge status="unknown" />
        <StatusBadge status="awaiting_verification" />
        <Badge>Neutral</Badge>
      </div>
      <Progress label="Bounded progress" value={64} />
      <Progress label="Unknown progress" />
      <Skeleton />
      <DataTable
        caption="Element table"
        rowKey={(r) => r.id}
        rows={[{ id: "1", value: "Example" }]}
        columns={[{ key: "value", label: "Value", render: (r) => r.value }]}
      />
      <EmptyState title="Empty state">Nothing has been inferred.</EmptyState>
      <ErrorState title="Example error">
        A truthful recoverable failure.
      </ErrorState>
    </div>
  );
}
