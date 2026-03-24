import { memo } from "react";
import { BaseEdge, getBezierPath, type EdgeProps } from "@xyflow/react";

interface DataFlowEdgeData {
  connectorType: string;
  label: string;
  [key: string]: unknown;
}

const CONNECTOR_COLORS: Record<string, string> = {
  "|": "var(--agent)",
  "&&": "var(--success)",
  "||": "var(--warning)",
  ";": "var(--text-secondary)",
  seq: "var(--text-secondary)",
};

export const DataFlowEdge = memo(function DataFlowEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  data,
  animated,
}: EdgeProps) {
  const d = data as DataFlowEdgeData;
  const [edgePath, labelX, labelY] = getBezierPath({
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
  });

  const color = CONNECTOR_COLORS[d.connectorType] ?? "var(--glass-border)";

  return (
    <>
      <BaseEdge
        id={id}
        path={edgePath}
        style={{
          stroke: color,
          strokeWidth: 2,
          opacity: 0.7,
          ...(animated ? {} : {}),
        }}
      />
      {d.label && (
        <foreignObject
          x={labelX - 20}
          y={labelY - 10}
          width={40}
          height={20}
          requiredExtensions="http://www.w3.org/1999/xhtml"
        >
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              width: "100%",
              height: "100%",
            }}
          >
            <span
              style={{
                fontSize: 9,
                fontWeight: 700,
                color,
                background: "var(--bg-primary)",
                padding: "1px 6px",
                borderRadius: 0,
                border: `1px solid ${color}`,
                opacity: 0.9,
              }}
            >
              {d.label}
            </span>
          </div>
        </foreignObject>
      )}
    </>
  );
});
