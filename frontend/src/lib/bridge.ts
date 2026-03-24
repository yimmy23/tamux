/**
 * Shared Electron bridge accessor -- single source of truth.
 * Replaces all (window as any).tamux ?? (window as any).amux casts.
 * Returns null when running outside Electron (e.g., in a browser or SSR).
 */
export function getBridge(): AmuxBridge | null {
    if (typeof window === "undefined") return null;
    return window.tamux ?? window.amux ?? null;
}
