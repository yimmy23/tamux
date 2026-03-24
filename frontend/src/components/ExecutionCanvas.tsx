import { useEffect, useMemo, useCallback, type CSSProperties } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  useNodesState,
  useEdgesState,
  BackgroundVariant,
  type Node,
  type Edge,
  type NodeTypes,
  type EdgeTypes,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";

import { useWorkspaceStore } from "../lib/workspaceStore";
import { useCommandLogStore } from "../lib/commandLogStore";
import { useGraphStore } from "../lib/graphStore";
import { ToolNode } from "./graph/ToolNode";
import { DataFlowEdge } from "./graph/DataFlowEdge";

const nodeTypes: NodeTypes = { toolNode: ToolNode };
const edgeTypes: EdgeTypes = { dataFlowEdge: DataFlowEdge };

/**
 * Execution Canvas — full-screen overlay showing a React Flow DAG
 * of executed command pipelines. Each command is parsed into tool nodes
 * connected by data flow edges (pipe, and, or, seq).
 */
type ExecutionCanvasProps = {
  style?: CSSProperties;
  className?: string;
};

export function ExecutionCanvas({ style, className }: ExecutionCanvasProps = {}) {
  const canvasOpen = useWorkspaceStore((s) => s.canvasOpen);
  const toggleCanvas = useWorkspaceStore((s) => s.toggleCanvas);
  const activePaneId = useWorkspaceStore((s) => s.activePaneId());
  const entries = useCommandLogStore((s) => s.entries);
  const buildFromEntries = useGraphStore((s) => s.buildFromEntries);
  const graphNodes = useGraphStore((s) => s.nodes);
  const graphEdges = useGraphStore((s) => s.edges);

  const [nodes, setNodes, onNodesChange] = useNodesState([] as Node[]);
  const [edges, setEdges, onEdgesChange] = useEdgesState([] as Edge[]);

  // Filter entries for active pane, limit to recent 20
  const scopedEntries = useMemo(() => {
    const filtered = activePaneId
      ? entries.filter((e) => e.paneId === activePaneId)
      : entries;
    return filtered.slice(0, 20);
  }, [entries, activePaneId]);

  // Build graph when entries change
  useEffect(() => {
    buildFromEntries(scopedEntries);
  }, [scopedEntries, buildFromEntries]);

  // Sync graph store → React Flow state
  useEffect(() => {
    setNodes(graphNodes);
    setEdges(graphEdges);
  }, [graphNodes, graphEdges, setNodes, setEdges]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Escape") toggleCanvas();
    },
    [toggleCanvas]
  );

  const pipeCount = useMemo(
    () => edges.filter((e) => (e.data as { connectorType?: string })?.connectorType === "|").length,
    [edges]
  );

  if (!canvasOpen) return null;

  return (
    <div
      onClick={toggleCanvas}
      onKeyDown={handleKeyDown}
      tabIndex={-1}
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(3,8,14,0.72)",
        display: "flex",
        alignItems: "flex-start",
        justifyContent: "center",
        padding: "4vh 2vw",
        zIndex: 940,
        backdropFilter: "none",
        ...(style ?? {}),
      }}
      className={className}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          background: "var(--bg-primary)",
          border: "1px solid var(--glass-border)",
          borderRadius: 0,
          width: "min(1500px, 96vw)",
          height: "min(900px, 88vh)",
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
        }}
        className="amux-shell-card"
      >
        {/* Header */}
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            padding: "16px 20px",
            borderBottom: "1px solid rgba(255,255,255,0.08)",
          }}
        >
          <div style={{ display: "grid", gap: 4 }}>
            <span className="amux-panel-title" style={{ color: "var(--agent)" }}>
              Execution Graph
            </span>
            <span style={{ fontSize: 20, fontWeight: 800 }}>Infinite Canvas</span>
          </div>
          <div style={{ display: "flex", gap: 12, alignItems: "center" }}>
            <div style={{ display: "flex", gap: 10 }}>
              <MetricChip label="Nodes" value={nodes.length} />
              <MetricChip label="Pipes" value={pipeCount} />
              <MetricChip label="Commands" value={scopedEntries.length} />
            </div>
            <button onClick={toggleCanvas} style={closeBtnStyle} title="Close (Esc)">
              ✕
            </button>
          </div>
        </div>

        {/* Canvas */}
        <div style={{ flex: 1, minHeight: 0 }}>
          {nodes.length === 0 ? (
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                height: "100%",
                color: "var(--text-secondary)",
                fontSize: 14,
              }}
            >
              <div style={{ textAlign: "center" }}>
                <div style={{ fontSize: 40, marginBottom: 12, opacity: 0.4 }}>◈</div>
                <div style={{ fontWeight: 600 }}>No execution data yet</div>
                <div style={{ fontSize: 12, marginTop: 6, opacity: 0.7 }}>
                  Run commands in the terminal to build the pipeline graph
                </div>
              </div>
            </div>
          ) : (
            <ReactFlow
              nodes={nodes}
              edges={edges}
              onNodesChange={onNodesChange}
              onEdgesChange={onEdgesChange}
              nodeTypes={nodeTypes}
              edgeTypes={edgeTypes}
              fitView
              fitViewOptions={{ padding: 0.3 }}
              minZoom={0.1}
              maxZoom={2}
              proOptions={{ hideAttribution: true }}
              style={{ background: "transparent" }}
            >
              <Background
                variant={BackgroundVariant.Dots}
                gap={20}
                size={1}
                color="rgba(255,255,255,0.04)"
              />
              <Controls
                position="bottom-left"
                style={{
                  background: "rgba(14, 24, 35, 0.9)",
                  border: "1px solid var(--glass-border)",
                  borderRadius: 0,
                }}
              />
              <MiniMap
                position="bottom-right"
                style={{
                  background: "rgba(14, 24, 35, 0.9)",
                  border: "1px solid var(--glass-border)",
                  borderRadius: 0,
                }}
                nodeColor={() => "var(--accent)"}
                maskColor="rgba(0, 0, 0, 0.5)"
              />
            </ReactFlow>
          )}
        </div>
      </div>
    </div>
  );
}

function MetricChip({ label, value }: { label: string; value: number }) {
  return (
    <div
      style={{
        padding: "4px 10px",
        borderRadius: 0,
        border: "1px solid rgba(255,255,255,0.08)",
        background: "rgba(255,255,255,0.03)",
        fontSize: 11,
        display: "flex",
        gap: 6,
        alignItems: "center",
      }}
    >
      <span style={{ color: "var(--text-secondary)" }}>{label}</span>
      <span style={{ fontWeight: 700, color: "var(--text-primary)" }}>{value}</span>
    </div>
  );
}

const closeBtnStyle: React.CSSProperties = {
  background: "rgba(255,255,255,0.04)",
  border: "1px solid rgba(255,255,255,0.08)",
  color: "var(--text-secondary)",
  cursor: "pointer",
  fontSize: 14,
  padding: "6px 10px",
  borderRadius: 0,
  lineHeight: 1,
};
