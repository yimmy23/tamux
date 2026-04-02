import type { Dispatch, SetStateAction } from "react";
import type { AgentRun } from "../../../lib/agentRuns";
import type { AgentQueueTask } from "../../../lib/agentTaskQueue";
import { SectionTitle, ActionButton } from "../shared";
import { TaskCard, TaskPostMortem } from "./TaskSection";
import { inputBlockStyle, inputRowStyle, sectionLabelStyle } from "./styles";
import type { ThreadTarget } from "./types";

interface TaskQueuePanelProps {
  newTaskTitle: string;
  setNewTaskTitle: Dispatch<SetStateAction<string>>;
  newTaskDescription: string;
  setNewTaskDescription: Dispatch<SetStateAction<string>>;
  newTaskCommand: string;
  setNewTaskCommand: Dispatch<SetStateAction<string>>;
  newTaskSessionId: string;
  setNewTaskSessionId: Dispatch<SetStateAction<string>>;
  newTaskDependencies: string;
  setNewTaskDependencies: Dispatch<SetStateAction<string>>;
  onAddTask: () => void;
  activeTasks: AgentQueueTask[];
  completedTasks: AgentQueueTask[];
  selectedTask: AgentQueueTask | null;
  selectedTaskSubagents: AgentRun[];
  onSelectTask: (taskId: string | null) => void;
  onCancelTask: (taskId: string) => void;
  onOpenTaskThread: (task: ThreadTarget) => void;
  hasTasks: boolean;
}

export function TaskQueuePanel({
  newTaskTitle,
  setNewTaskTitle,
  newTaskDescription,
  setNewTaskDescription,
  newTaskCommand,
  setNewTaskCommand,
  newTaskSessionId,
  setNewTaskSessionId,
  newTaskDependencies,
  setNewTaskDependencies,
  onAddTask,
  activeTasks,
  completedTasks,
  selectedTask,
  selectedTaskSubagents,
  onSelectTask,
  onCancelTask,
  onOpenTaskThread,
  hasTasks,
}: TaskQueuePanelProps) {
  return (
    <>
      <SectionTitle
        title="Task Queue"
        subtitle="Autonomous task execution by daemon agent"
      />

      <div
        style={{
          marginBottom: "var(--space-4)",
          display: "flex",
          flexDirection: "column",
          gap: "var(--space-2)",
        }}
      >
        <input
          type="text"
          placeholder="Task title..."
          value={newTaskTitle}
          onChange={(event) => setNewTaskTitle(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              onAddTask();
            }
          }}
          style={inputRowStyle}
        />
        {newTaskTitle && (
          <textarea
            placeholder="Description (optional)..."
            value={newTaskDescription}
            onChange={(event) => setNewTaskDescription(event.target.value)}
            rows={2}
            style={inputBlockStyle}
          />
        )}
        {newTaskTitle && (
          <input
            type="text"
            placeholder="Preferred command or entrypoint (optional)..."
            value={newTaskCommand}
            onChange={(event) => setNewTaskCommand(event.target.value)}
            style={inputRowStyle}
          />
        )}
        {newTaskTitle && (
          <input
            type="text"
            placeholder="Target session ID (optional)..."
            value={newTaskSessionId}
            onChange={(event) => setNewTaskSessionId(event.target.value)}
            style={inputRowStyle}
          />
        )}
        {newTaskTitle && (
          <input
            type="text"
            placeholder="Dependencies: task IDs, comma-separated (optional)..."
            value={newTaskDependencies}
            onChange={(event) => setNewTaskDependencies(event.target.value)}
            style={inputRowStyle}
          />
        )}
        {newTaskTitle && (
          <ActionButton onClick={onAddTask}>Add Task</ActionButton>
        )}
      </div>

      {activeTasks.length > 0 && (
        <div style={{ marginBottom: "var(--space-4)" }}>
          <div style={sectionLabelStyle}>Active ({activeTasks.length})</div>
          {activeTasks.map((task) => (
            <TaskCard
              key={task.id}
              task={task}
              selected={task.id === selectedTask?.id}
              onSelect={() => onSelectTask(task.id)}
              onCancel={() => onCancelTask(task.id)}
            />
          ))}
        </div>
      )}

      {completedTasks.length > 0 && (
        <div style={{ marginBottom: "var(--space-4)" }}>
          <div style={sectionLabelStyle}>History ({completedTasks.length})</div>
          {completedTasks.slice(0, 20).map((task) => (
            <TaskCard
              key={task.id}
              task={task}
              selected={task.id === selectedTask?.id}
              onSelect={() => onSelectTask(task.id)}
            />
          ))}
        </div>
      )}

      {!hasTasks && (
        <div
          style={{
            textAlign: "center",
            padding: "var(--space-6)",
            color: "var(--text-muted)",
            fontSize: "var(--text-sm)",
          }}
        >
          No tasks yet. Add a task above or let a goal runner enqueue child work.
        </div>
      )}

      {selectedTask && (
        <div style={{ marginBottom: "var(--space-5)" }}>
          <SectionTitle
            title="Post-Mortem"
            subtitle="Latest trajectory for the selected task"
          />
          <TaskPostMortem
            task={selectedTask}
            subagents={selectedTaskSubagents}
            onSelectTask={onSelectTask}
            onOpenTaskThread={onOpenTaskThread}
          />
        </div>
      )}
    </>
  );
}
