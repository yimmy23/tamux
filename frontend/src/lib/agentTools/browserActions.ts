import { getBridge } from "../bridge";
import { getBrowserController } from "../browserRegistry";
import { getCanvasBrowserController } from "../canvasBrowserRegistry";
import type { ToolResult } from "./types";
import { resolvePaneIdByRef } from "./workspaceHelpers";

function resolveCanvasBrowser(
  callId: string,
  name: string,
  paneRef?: string,
): { ctrl: NonNullable<ReturnType<typeof getCanvasBrowserController>>; paneId: string } | ToolResult {
  if (!paneRef?.trim()) {
    return { toolCallId: callId, name, content: "Error: pane parameter is required for browser-use tools." };
  }
  const paneId = resolvePaneIdByRef(paneRef);
  if (!paneId) {
    return { toolCallId: callId, name, content: `Error: Pane not found for "${paneRef}".` };
  }
  const ctrl = getCanvasBrowserController(paneId);
  if (!ctrl) {
    return { toolCallId: callId, name, content: `Error: Pane "${paneRef}" is not a browser panel or is not mounted.` };
  }
  return { ctrl, paneId };
}

function isToolResult(value: unknown): value is ToolResult {
  return typeof value === "object" && value !== null && "toolCallId" in value;
}

export async function executeBrowserNavigate(callId: string, name: string, url?: string, paneRef?: string): Promise<ToolResult> {
  if (!url?.trim()) {
    return { toolCallId: callId, name, content: "Error: URL is required." };
  }
  if (paneRef?.trim()) {
    const paneId = resolvePaneIdByRef(paneRef);
    if (!paneId) return { toolCallId: callId, name, content: `Error: Pane not found for "${paneRef}".` };
    const ctrl = getCanvasBrowserController(paneId);
    if (!ctrl) return { toolCallId: callId, name, content: `Error: Pane ${paneId} is not a browser panel or is not mounted yet.` };
    ctrl.navigate(url);
    return { toolCallId: callId, name, content: `Canvas browser [${paneId}] navigating to ${url}.` };
  }
  const browser = getBrowserController();
  if (!browser) return { toolCallId: callId, name, content: "Error: Browser panel is not available. Use open_canvas_browser to create one on a canvas." };
  await browser.navigate(url);
  return { toolCallId: callId, name, content: `Navigated browser to ${url}.` };
}

export async function executeBrowserBack(callId: string, name: string): Promise<ToolResult> {
  const browser = getBrowserController();
  if (!browser) return { toolCallId: callId, name, content: "Error: Browser panel is not available." };
  await browser.back();
  return { toolCallId: callId, name, content: "Browser navigated back." };
}

export async function executeBrowserForward(callId: string, name: string): Promise<ToolResult> {
  const browser = getBrowserController();
  if (!browser) return { toolCallId: callId, name, content: "Error: Browser panel is not available." };
  await browser.forward();
  return { toolCallId: callId, name, content: "Browser navigated forward." };
}

export async function executeBrowserReload(callId: string, name: string): Promise<ToolResult> {
  const browser = getBrowserController();
  if (!browser) return { toolCallId: callId, name, content: "Error: Browser panel is not available." };
  await browser.reload();
  return { toolCallId: callId, name, content: "Browser reloaded." };
}

export async function executeBrowserReadDom(callId: string, name: string, paneRef?: string): Promise<ToolResult> {
  if (paneRef?.trim()) {
    const paneId = resolvePaneIdByRef(paneRef);
    if (!paneId) return { toolCallId: callId, name, content: `Error: Pane not found for "${paneRef}".` };
    const ctrl = getCanvasBrowserController(paneId);
    if (!ctrl) return { toolCallId: callId, name, content: `Error: Pane ${paneId} is not a browser panel or not mounted.` };
    const snapshot = await ctrl.getDomSnapshot();
    const text = snapshot.text || "(empty DOM text)";
    const preview = text.length > 12000 ? `${text.slice(0, 12000)}\n\n[truncated]` : text;
    return { toolCallId: callId, name, content: `URL: ${snapshot.url}\nTitle: ${snapshot.title}\n\nDOM text:\n${preview}` };
  }
  const browser = getBrowserController();
  if (!browser) return { toolCallId: callId, name, content: "Error: Browser panel is not available." };
  const snapshot = await browser.getDomSnapshot();
  const text = snapshot.text || "(empty DOM text)";
  const preview = text.length > 12000 ? `${text.slice(0, 12000)}\n\n[truncated]` : text;
  return { toolCallId: callId, name, content: `URL: ${snapshot.url}\nTitle: ${snapshot.title}\n\nDOM text:\n${preview}` };
}

export async function executeBrowserScreenshot(callId: string, name: string): Promise<ToolResult> {
  const browser = getBrowserController();
  if (!browser) return { toolCallId: callId, name, content: "Error: Browser panel is not available." };
  const shot = await browser.captureScreenshot();
  const amux = getBridge();
  if (!amux?.saveVisionScreenshot) {
    return { toolCallId: callId, name, content: "Error: Vision screenshot persistence is not available in this environment." };
  }
  const saved = await amux.saveVisionScreenshot({ dataUrl: shot.dataUrl });
  if (!saved?.ok) {
    return { toolCallId: callId, name, content: `Error: Failed to save screenshot: ${saved?.error || "unknown error"}` };
  }
  return {
    toolCallId: callId,
    name,
    content: `Screenshot saved: ${saved.path}\nExpiresAt: ${saved.expiresAt ? new Date(saved.expiresAt).toISOString() : "unknown"}\nPage: ${shot.title || "(untitled)"}\nURL: ${shot.url}`,
  };
}

export async function executeBrowserClick(callId: string, name: string, paneRef?: string, selector?: string, text?: string): Promise<ToolResult> {
  const resolved = resolveCanvasBrowser(callId, name, paneRef);
  if (isToolResult(resolved)) return resolved;
  if (!selector && !text) return { toolCallId: callId, name, content: "Error: Provide either a CSS selector or text to match." };

  try {
    const script = selector
      ? `(() => {
          const el = document.querySelector(${JSON.stringify(selector)});
          if (!el) return { ok: false, error: 'Element not found: ' + ${JSON.stringify(selector)} };
          el.scrollIntoView({ block: 'center' });
          el.click();
          return { ok: true, tag: el.tagName, text: (el.textContent || '').slice(0, 100) };
        })()`
      : `(() => {
          const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_ELEMENT);
          const target = ${JSON.stringify(text)}.toLowerCase();
          let node;
          while ((node = walker.nextNode())) {
            const el = node;
            if (el.children.length === 0 || el.tagName === 'A' || el.tagName === 'BUTTON') {
              if ((el.textContent || '').trim().toLowerCase().includes(target)) {
                el.scrollIntoView({ block: 'center' });
                el.click();
                return { ok: true, tag: el.tagName, text: (el.textContent || '').slice(0, 100) };
              }
            }
          }
          return { ok: false, error: 'No element found containing text: ' + ${JSON.stringify(text)} };
        })()`;
    const result = await resolved.ctrl.executeJavaScript(script) as any;
    if (!result?.ok) return { toolCallId: callId, name, content: `Error: ${result?.error || "Click failed"}` };
    return { toolCallId: callId, name, content: `Clicked <${result.tag}> "${result.text}"` };
  } catch (error: any) {
    return { toolCallId: callId, name, content: `Error: ${error.message || String(error)}` };
  }
}

export async function executeBrowserType(callId: string, name: string, paneRef?: string, selector?: string, text?: string, clear?: boolean): Promise<ToolResult> {
  const resolved = resolveCanvasBrowser(callId, name, paneRef);
  if (isToolResult(resolved)) return resolved;
  if (!selector || !text) return { toolCallId: callId, name, content: "Error: Both selector and text are required." };

  try {
    const shouldClear = clear !== false;
    const script = `(() => {
      const el = document.querySelector(${JSON.stringify(selector)});
      if (!el) return { ok: false, error: 'Element not found: ' + ${JSON.stringify(selector)} };
      el.focus();
      ${shouldClear ? `
      if ('value' in el) { el.value = ''; }
      else if (el.isContentEditable) { el.textContent = ''; }
      ` : ""}
      if ('value' in el) {
        const nativeSet = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el).constructor.prototype, 'value')?.set;
        if (nativeSet) { nativeSet.call(el, ${JSON.stringify(text)}); }
        else { el.value = ${JSON.stringify(text)}; }
        el.dispatchEvent(new Event('input', { bubbles: true }));
        el.dispatchEvent(new Event('change', { bubbles: true }));
      } else if (el.isContentEditable) {
        el.textContent = ${JSON.stringify(text)};
        el.dispatchEvent(new Event('input', { bubbles: true }));
      }
      return { ok: true, tag: el.tagName };
    })()`;
    const result = await resolved.ctrl.executeJavaScript(script) as any;
    if (!result?.ok) return { toolCallId: callId, name, content: `Error: ${result?.error || "Type failed"}` };
    return { toolCallId: callId, name, content: `Typed into <${result.tag}> "${selector}"` };
  } catch (error: any) {
    return { toolCallId: callId, name, content: `Error: ${error.message || String(error)}` };
  }
}

export async function executeBrowserScroll(callId: string, name: string, paneRef?: string, direction?: string, amount?: number, selector?: string): Promise<ToolResult> {
  const resolved = resolveCanvasBrowser(callId, name, paneRef);
  if (isToolResult(resolved)) return resolved;
  const pixels = amount || 400;
  const delta = direction === "up" ? -pixels : pixels;

  try {
    const script = selector
      ? `(() => {
          const el = document.querySelector(${JSON.stringify(selector)});
          if (!el) return { ok: false, error: 'Element not found' };
          el.scrollBy(0, ${delta});
          return { ok: true, scrollTop: el.scrollTop };
        })()`
      : `(() => { window.scrollBy(0, ${delta}); return { ok: true, scrollY: window.scrollY }; })()`;
    const result = await resolved.ctrl.executeJavaScript(script) as any;
    if (!result?.ok) return { toolCallId: callId, name, content: `Error: ${result?.error || "Scroll failed"}` };
    return { toolCallId: callId, name, content: `Scrolled ${direction} by ${pixels}px. Position: ${result.scrollY ?? result.scrollTop}` };
  } catch (error: any) {
    return { toolCallId: callId, name, content: `Error: ${error.message || String(error)}` };
  }
}

export async function executeBrowserGetElements(callId: string, name: string, paneRef?: string, filter?: string, limit?: number): Promise<ToolResult> {
  const resolved = resolveCanvasBrowser(callId, name, paneRef);
  if (isToolResult(resolved)) return resolved;
  const maxItems = Math.min(limit || 50, 200);
  const filterType = filter || "all";

  try {
    const script = `(() => {
      const filterType = ${JSON.stringify(filterType)};
      const selectors = {
        links: 'a[href]',
        buttons: 'button, [role="button"], input[type="submit"], input[type="button"]',
        inputs: 'input:not([type="hidden"]), textarea, select, [contenteditable="true"]',
        all: 'a[href], button, [role="button"], input:not([type="hidden"]), textarea, select, [contenteditable="true"]',
      };
      const sel = selectors[filterType] || selectors.all;
      const els = Array.from(document.querySelectorAll(sel)).slice(0, ${maxItems});
      return els.map((el) => {
        const rect = el.getBoundingClientRect();
        const visible = rect.width > 0 && rect.height > 0 && rect.top < window.innerHeight && rect.bottom > 0;
        if (!visible) return null;
        const text = (el.textContent || '').trim().slice(0, 80);
        const tag = el.tagName.toLowerCase();
        const href = el.getAttribute('href') || '';
        const type = el.getAttribute('type') || '';
        const placeholder = el.getAttribute('placeholder') || '';
        const id = el.id ? '#' + el.id : '';
        const cls = el.className && typeof el.className === 'string' ? '.' + el.className.split(' ')[0] : '';
        const hint = tag + id + cls;
        return { tag, text, href, type, placeholder, hint };
      }).filter(Boolean);
    })()`;
    const result = await resolved.ctrl.executeJavaScript(script) as any[];
    if (!result || result.length === 0) return { toolCallId: callId, name, content: `No ${filterType} elements found on the page.` };

    const lines = result.map((element: any) => {
      const parts = [`<${element.tag}>`];
      if (element.text) parts.push(`"${element.text}"`);
      if (element.href) parts.push(`href=${element.href}`);
      if (element.type) parts.push(`type=${element.type}`);
      if (element.placeholder) parts.push(`placeholder="${element.placeholder}"`);
      parts.push(`selector="${element.hint}"`);
      return parts.join(" ");
    });
    return { toolCallId: callId, name, content: `Found ${result.length} ${filterType} elements:\n${lines.join("\n")}` };
  } catch (error: any) {
    return { toolCallId: callId, name, content: `Error: ${error.message || String(error)}` };
  }
}

export async function executeBrowserEvalJs(callId: string, name: string, paneRef?: string, code?: string): Promise<ToolResult> {
  const resolved = resolveCanvasBrowser(callId, name, paneRef);
  if (isToolResult(resolved)) return resolved;
  if (!code?.trim()) return { toolCallId: callId, name, content: "Error: code parameter is required." };

  try {
    const result = await resolved.ctrl.executeJavaScript(code);
    const output = result === undefined ? "(undefined)" : JSON.stringify(result, null, 2);
    const maxChars = 12000;
    const truncated = output.length > maxChars ? `${output.slice(0, maxChars)}\n\n[truncated to ${maxChars} chars]` : output;
    return { toolCallId: callId, name, content: truncated };
  } catch (error: any) {
    return { toolCallId: callId, name, content: `Error: ${error.message || String(error)}` };
  }
}
