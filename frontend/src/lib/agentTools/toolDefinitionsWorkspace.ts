import type { ToolDefinition } from "./types";

export const WORKSPACE_TOOLS: ToolDefinition[] = [
  { type: "function", function: { name: "list_workspaces", description: "List workspaces, surfaces, and panes (with pane names and IDs).", parameters: { type: "object", properties: {} } } },
  {
    type: "function",
    function: {
      name: "create_workspace",
      description: "Create a new workspace and make it active.",
      parameters: { type: "object", properties: { name: { type: "string", description: "Optional workspace name" } } },
    },
  },
  {
    type: "function",
    function: {
      name: "set_active_workspace",
      description: "Set the active workspace by workspace ID or name.",
      parameters: {
        type: "object",
        properties: { workspace: { type: "string", description: "Workspace ID or exact workspace name" } },
        required: ["workspace"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "create_surface",
      description: "Create a new surface (tab) in a workspace; defaults to active workspace.",
      parameters: {
        type: "object",
        properties: {
          workspace: { type: "string", description: "Optional workspace ID or exact name" },
          name: { type: "string", description: "Optional new surface name" },
        },
      },
    },
  },
  {
    type: "function",
    function: {
      name: "set_active_surface",
      description: "Set active surface by surface ID or exact surface name.",
      parameters: {
        type: "object",
        properties: {
          surface: { type: "string", description: "Surface ID or exact surface name" },
          workspace: { type: "string", description: "Optional workspace ID or exact name to scope surface lookup" },
        },
        required: ["surface"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "split_pane",
      description: "Split a pane horizontally or vertically. Defaults to active pane.",
      parameters: {
        type: "object",
        properties: {
          direction: { type: "string", enum: ["horizontal", "vertical"], description: "Split direction" },
          pane: { type: "string", description: "Optional pane ID or pane name to split" },
          new_pane_name: { type: "string", description: "Optional name for the newly created pane" },
        },
        required: ["direction"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "rename_pane",
      description: "Rename a pane by ID or exact pane name. Defaults to active pane if omitted.",
      parameters: {
        type: "object",
        properties: {
          pane: { type: "string", description: "Optional pane ID or exact pane name" },
          name: { type: "string", description: "New pane name" },
        },
        required: ["name"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "set_layout_preset",
      description: "Apply a layout preset to a surface.",
      parameters: {
        type: "object",
        properties: {
          preset: { type: "string", enum: ["single", "2-columns", "3-columns", "grid-2x2", "main-stack"], description: "Layout preset" },
          surface: { type: "string", description: "Optional surface ID or exact name" },
          workspace: { type: "string", description: "Optional workspace ID or exact name to scope surface lookup" },
        },
        required: ["preset"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "equalize_layout",
      description: "Equalize all split ratios in the active (or targeted) surface.",
      parameters: {
        type: "object",
        properties: {
          surface: { type: "string", description: "Optional surface ID or exact name" },
          workspace: { type: "string", description: "Optional workspace ID or exact name to scope surface lookup" },
        },
      },
    },
  },
  {
    type: "function",
    function: {
      name: "list_snippets",
      description: "List snippets (name, owner, category, content preview). Optional owner filter: user, assistant, or both.",
      parameters: {
        type: "object",
        properties: {
          owner: { type: "string", enum: ["user", "assistant", "both"], description: "Optional snippet owner filter" },
        },
      },
    },
  },
  {
    type: "function",
    function: {
      name: "create_snippet",
      description: "Create a new snippet owned by the assistant.",
      parameters: {
        type: "object",
        properties: {
          name: { type: "string", description: "Snippet name" },
          content: { type: "string", description: "Snippet command/template content" },
          category: { type: "string", description: "Optional category" },
          description: { type: "string", description: "Optional description" },
          tags: { type: "array", items: { type: "string" }, description: "Optional tags" },
        },
        required: ["name", "content"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "run_snippet",
      description: "Execute a snippet by id or exact name in a pane.",
      parameters: {
        type: "object",
        properties: {
          snippet: { type: "string", description: "Snippet ID or exact snippet name" },
          pane: { type: "string", description: "Optional pane ID or pane name; defaults to active pane" },
          params: { type: "object", additionalProperties: { type: "string" }, description: "Optional template parameters for placeholders like {{name}}" },
          execute: { type: "boolean", description: "When true (default), append Enter after inserting snippet content" },
        },
        required: ["snippet"],
      },
    },
  },
];
