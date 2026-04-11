import type React from "react";
import type { AgentMessage, AgentThread, AgentTodoItem } from "../../../lib/agentStore";
import type { WelesReviewMeta } from "../../../lib/agentTools";
import type { WelesHealthState } from "../welesHealthPresentation";

export type ChatViewProps = {
  messages: AgentMessage[];
  todos: AgentTodoItem[];
  input: string;
  setInput: (value: string) => void;
  inputRef: React.RefObject<HTMLTextAreaElement | null>;
  onKeyDown: (event: React.KeyboardEvent) => void;
  agentSettings: { enabled: boolean; chatFontFamily: string; reasoning_effort: string };
  isStreamingResponse: boolean;
  activeThread: AgentThread | undefined;
  messagesEndRef: React.RefObject<HTMLDivElement | null>;
  onSendMessage: (text: string) => void;
  onSendParticipantSuggestion: (threadId: string, suggestionId: string, forceSend?: boolean) => void | Promise<void>;
  onDismissParticipantSuggestion: (threadId: string, suggestionId: string) => void | Promise<void>;
  onStopStreaming: () => void;
  onDeleteMessage?: (messageId: string) => void;
  onUpdateReasoningEffort: (value: string) => void;
  canStartGoalRun: boolean;
  onStartGoalRun: (text: string) => Promise<boolean>;
  welesHealth?: WelesHealthState | null;
};

export type ToolEventGroup = {
  key: string;
  toolCallId: string;
  toolName: string;
  toolArguments: string;
  status: "requested" | "executing" | "done" | "error";
  resultContent: string;
  createdAt: number;
  welesReview?: WelesReviewMeta;
};

export type ChatDisplayItem =
  | { type: "message"; message: AgentMessage }
  | { type: "tool"; group: ToolEventGroup };
