import type { RemoteAgentMessageRecord } from "../../../lib/agentStore";

export interface HeartbeatItem {
  id: string;
  label: string;
  prompt: string;
  interval_minutes: number;
  enabled: boolean;
  last_run_at: number | null;
  last_result: "ok" | "alert" | "error" | null;
  last_message: string | null;
}

export type TaskWorkspaceLocation = {
  workspaceId: string;
  workspaceName: string;
  surfaceId: string;
  surfaceName: string;
  paneId: string;
  cwd: string | null;
};

export type TasksViewProps = {
  onOpenThreadView?: () => void;
};

export type RemoteAgentThread = {
  id: string;
  title: string;
  messages: RemoteAgentMessageRecord[];
};

export type ThreadTarget = {
  title: string;
  thread_id?: string | null;
  session_id?: string | null;
};
