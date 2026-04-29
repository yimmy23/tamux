export function shouldAutoStartOperatorProfileFromConcierge(event: unknown): boolean {
  if (!event || typeof event !== "object") {
    return false;
  }
  const actions = (event as { actions?: unknown }).actions;
  return Array.isArray(actions) && actions.length > 0;
}
