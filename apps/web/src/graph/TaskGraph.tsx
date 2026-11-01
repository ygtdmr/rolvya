import { memo, useMemo, useState } from "react";
import {
  Background,
  Handle,
  Position,
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
  type Node,
  type NodeProps,
  type NodeTypes,
} from "@xyflow/react";
import type { Task } from "../api/model";
import { Button } from "../ui/controls";
import { Badge, StatusBadge } from "../ui/data";
type TaskNode = Node<{ task: Task; selectedTask: boolean }, "task">;
export const GraphTaskNode = memo(function GraphTaskNode({
  data,
}: NodeProps<TaskNode>) {
  const t = data.task;
  return (
    <div
      className={`rv-graph-node ${t.kind === "group" ? "rv-graph-group" : ""}`}
      data-task={t.id}
      data-selected={data.selectedTask}
    >
      <Handle type="target" position={Position.Left} isConnectable={false} />
      <Handle type="source" position={Position.Right} isConnectable={false} />
      <div className="rv-graph-node-heading">
        <span className="rv-task-letter" data-kind={t.kind}>
          {t.id}
        </span>
        <span>
          {t.kind === "group" ? "VERIFICATION GROUP" : "EXECUTABLE LEAF"}
        </span>
      </div>
      <strong>{t.title}</strong>
      {t.kind === "group" ? (
        <small>Required descendants · no worker</small>
      ) : (
        <>
          <StatusBadge status={t.status} />
          <small>{t.model ?? "Unassigned"}</small>
        </>
      )}
    </div>
  );
});
const types: NodeTypes = { task: GraphTaskNode };
export const GRAPH_LIMIT = 200;
export function graphNodes(
  tasks: Task[],
  selected: string | null,
  limit = GRAPH_LIMIT,
): TaskNode[] {
  const shown = tasks.slice(0, limit),
    ids = new Set(shown.map((t) => t.id)),
    children = new Map<string, Task[]>();
  for (const t of shown) {
    const parent = t.parent && ids.has(t.parent) ? t.parent : "";
    const siblings = children.get(parent) ?? [];
    siblings.push(t);
    children.set(parent, siblings);
  }
  const nodes: TaskNode[] = [];
  function place(
    task: Task,
    x: number,
    y: number,
    parent: string | undefined,
    seen: Set<string>,
  ): { width: number; height: number } {
    if (seen.has(task.id)) return { width: 222, height: 150 };
    const branch = new Set(seen).add(task.id),
      nested = children.get(task.id) ?? [];
    const start = nodes.length;
    nodes.push({
      id: task.id,
      type: "task",
      data: { task, selectedTask: task.id === selected },
      position: { x, y },
      ...(parent ? { parentId: parent, extent: "parent" as const } : {}),
      draggable: false,
      connectable: false,
      focusable: false,
    });
    let width = 222,
      height = 150;
    if (task.kind === "group") {
      let rowY = 100,
        rowHeight = 0,
        rowX = 24;
      nested.forEach((child, index) => {
        if (index % 2 === 0 && index > 0) {
          rowY += rowHeight + 24;
          rowX = 24;
          rowHeight = 0;
        }
        const size = place(child, rowX, rowY, task.id, branch);
        rowX += size.width + 24;
        rowHeight = Math.max(rowHeight, size.height);
        width = Math.max(width, rowX);
      });
      width = Math.max(width, 300);
      height = Math.max(190, rowY + rowHeight + 24);
    }
    const node = nodes[start];
    if (node) node.style = { width, height };
    return { width, height };
  }
  let x = 20,
    y = 20,
    rowHeight = 0;
  (children.get("") ?? []).forEach((task, index) => {
    if (index > 0 && index % 2 === 0) {
      x = 20;
      y += rowHeight + 80;
      rowHeight = 0;
    }
    const size = place(task, x, y, undefined, new Set());
    x += size.width + 100;
    rowHeight = Math.max(rowHeight, size.height);
  });
  return nodes;
}
function Canvas({
  tasks,
  selected,
  onSelect,
}: {
  tasks: Task[];
  selected: string | null;
  onSelect: (id: string) => void;
}) {
  const flow = useReactFlow();
  const [interactive, setInteractive] = useState(false);
  // Deliberately separate stress harness; production always uses the bounded preview.
  const limit =
    __DEMO__ && new URLSearchParams(location.search).get("profile-full") === "1"
      ? 5000
      : GRAPH_LIMIT;
  const nodes = useMemo(
    () => graphNodes(tasks, selected, limit),
    [tasks, selected, limit],
  );
  const edges = useMemo(() => {
    const ids = new Set(nodes.map((n) => n.id));
    return tasks.flatMap((t) =>
      t.dependencies
        .filter((d) => ids.has(d) && ids.has(t.id))
        .map((d) => ({
          id: `${d}-${t.id}`,
          source: d,
          target: t.id,
          type: "smoothstep",
          animated: false,
        })),
    );
  }, [tasks, nodes]);
  return (
    <div className="rv-graph" data-ui="dependency-graph">
      <div className="rv-graph-toolbar">
        <Badge icon="lock">Read-only DAG</Badge>
        <div>
          <Button
            variant="ghost"
            icon="minus"
            aria-label="Zoom out graph"
            onClick={() => {
              void flow.zoomOut();
            }}
          >
            Out
          </Button>
          <Button
            variant="ghost"
            icon="plus"
            aria-label="Zoom in graph"
            onClick={() => {
              void flow.zoomIn();
            }}
          >
            In
          </Button>
          <Button
            variant="ghost"
            icon="fit"
            onClick={() => {
              void flow.fitView({ padding: 0.14 });
            }}
          >
            Fit
          </Button>
          <Button
            variant="secondary"
            aria-pressed={interactive}
            onClick={() => setInteractive(!interactive)}
          >
            {interactive ? "Lock pan" : "Enable pan"}
          </Button>
        </div>
      </div>
      <div
        className="rv-graph-canvas"
        aria-label="Read-only dependency graph; use task hierarchy for keyboard inspection"
      >
        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={types}
          fitView
          fitViewOptions={{ padding: 0.14 }}
          minZoom={0.15}
          maxZoom={1.5}
          nodesDraggable={false}
          nodesConnectable={false}
          nodesFocusable={false}
          edgesFocusable={false}
          elementsSelectable={false}
          panOnDrag={interactive}
          zoomOnScroll={false}
          zoomOnPinch={interactive}
          zoomOnDoubleClick={false}
          preventScrolling={false}
          onNodeClick={(_, node) => onSelect(node.id)}
          proOptions={{ hideAttribution: false }}
        >
          <Background gap={24} size={1} />
        </ReactFlow>
      </div>
      <div className="rv-graph-caption">
        <span>
          <i className="rv-legend-line" /> Dependency · accepted predecessor
          required
        </span>
        <span>
          <i className="rv-legend-group" /> Group boundary · hierarchy only
        </span>
      </div>
      {tasks.length > limit && (
        <p className="rv-note">
          Graph preview is bounded to {limit} of {tasks.length} nodes. Search
          the complete paginated task list for all tasks. No topology is
          changed.
        </p>
      )}
    </div>
  );
}
export function TaskGraph(props: Parameters<typeof Canvas>[0]) {
  return (
    <ReactFlowProvider>
      <Canvas {...props} />
    </ReactFlowProvider>
  );
}
