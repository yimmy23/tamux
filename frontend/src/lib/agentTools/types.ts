export type WelesVerdict = "allow" | "block" | "flag_only";

export interface WelesReviewMeta {
  weles_reviewed: boolean;
  verdict: WelesVerdict;
  reasons: string[];
  audit_id?: string;
  security_override_mode?: string;
}

export interface ToolDefinition {
  type: "function";
  function: {
    name: string;
    description: string;
    parameters: Record<string, unknown>;
  };
}

export interface ToolCall {
  id: string;
  type: "function";
  function: {
    name: string;
    arguments: string;
  };
  weles_review?: WelesReviewMeta;
}

export interface ToolResult {
  toolCallId: string;
  name: string;
  content: string;
  weles_review?: WelesReviewMeta;
}
