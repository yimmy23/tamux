/**
 * Shared Electron bridge accessor -- single source of truth.
 * Replaces all (window as any).zorai ?? (window as any).zorai casts.
 * Returns null when running outside Electron (e.g., in a browser or SSR).
 */
export function getBridge(): ZoraiBridge | null {
    if (typeof window === "undefined") return null;
    return window.zorai ?? null;
}
