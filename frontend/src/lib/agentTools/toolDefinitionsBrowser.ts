import type { ToolDefinition } from "./types";

export const WEB_BROWSING_TOOLS: ToolDefinition[] = [
  {
    type: "function",
    function: {
      name: "open_canvas_browser",
      description: "Open a new browser panel on the active canvas surface. Returns the new pane ID for subsequent browser_navigate calls.",
      parameters: {
        type: "object",
        properties: {
          url: { type: "string", description: "Initial URL to load (default: https://google.com)" },
          name: { type: "string", description: "Optional panel name" },
        },
      },
    },
  },
  {
    type: "function",
    function: {
      name: "browser_navigate",
      description: "Navigate a browser to a URL. Without a pane parameter, uses the sidebar browser. With a pane ID/name, targets a specific canvas browser panel.",
      parameters: {
        type: "object",
        properties: {
          url: { type: "string", description: "URL to open (https://...)" },
          pane: { type: "string", description: "Optional canvas browser pane ID or name to target. If omitted, uses the sidebar browser." },
        },
        required: ["url"],
      },
    },
  },
  { type: "function", function: { name: "browser_back", description: "Navigate back in the sidebar browser history.", parameters: { type: "object", properties: {} } } },
  { type: "function", function: { name: "browser_forward", description: "Navigate forward in the sidebar browser history.", parameters: { type: "object", properties: {} } } },
  { type: "function", function: { name: "browser_reload", description: "Reload the sidebar browser page.", parameters: { type: "object", properties: {} } } },
];

export const VISION_TOOLS: ToolDefinition[] = [
  {
    type: "function",
    function: {
      name: "browser_read_dom",
      description: "Read current page DOM text/title/url from a browser. Without a pane parameter, uses the sidebar browser. With a pane ID/name, targets a canvas browser panel.",
      parameters: {
        type: "object",
        properties: {
          pane: { type: "string", description: "Optional canvas browser pane ID or name" },
        },
      },
    },
  },
  {
    type: "function",
    function: {
      name: "browser_take_screenshot",
      description: "Capture a browser screenshot, save it to temporary vision storage, and return its path.",
      parameters: { type: "object", properties: {} },
    },
  },
];

export const BROWSER_USE_TOOLS: ToolDefinition[] = [
  {
    type: "function",
    function: {
      name: "browser_click",
      description: "Click an element in a canvas browser panel. Target by CSS selector or visible text content.",
      parameters: {
        type: "object",
        properties: {
          pane: { type: "string", description: "Canvas browser pane ID or name" },
          selector: { type: "string", description: "CSS selector of the element to click" },
          text: { type: "string", description: "Visible text content to match when selector is not provided." },
        },
        required: ["pane"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "browser_type",
      description: "Type text into an input, textarea, or contenteditable element in a canvas browser panel.",
      parameters: {
        type: "object",
        properties: {
          pane: { type: "string", description: "Canvas browser pane ID or name" },
          selector: { type: "string", description: "CSS selector of the input element" },
          text: { type: "string", description: "Text to type" },
          clear: { type: "boolean", description: "Clear existing content before typing (default: true)" },
        },
        required: ["pane", "selector", "text"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "browser_scroll",
      description: "Scroll the page or a specific element in a canvas browser panel.",
      parameters: {
        type: "object",
        properties: {
          pane: { type: "string", description: "Canvas browser pane ID or name" },
          direction: { type: "string", enum: ["up", "down"], description: "Scroll direction" },
          amount: { type: "number", description: "Pixels to scroll (default: 400)" },
          selector: { type: "string", description: "Optional CSS selector of element to scroll (defaults to window)" },
        },
        required: ["pane", "direction"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "browser_get_elements",
      description: "List interactive elements visible on the current page in a canvas browser panel. Returns element tag, text, href, selector hint.",
      parameters: {
        type: "object",
        properties: {
          pane: { type: "string", description: "Canvas browser pane ID or name" },
          filter: { type: "string", description: "Optional filter: 'links', 'buttons', 'inputs', or 'all' (default: 'all')" },
          limit: { type: "number", description: "Max number of elements to return (default: 50)" },
        },
        required: ["pane"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "browser_eval_js",
      description: "Execute JavaScript code in the page context of a canvas browser panel and return the result.",
      parameters: {
        type: "object",
        properties: {
          pane: { type: "string", description: "Canvas browser pane ID or name" },
          code: { type: "string", description: "JavaScript code to evaluate in the page context. The return value is serialized as JSON." },
        },
        required: ["pane", "code"],
      },
    },
  },
];
