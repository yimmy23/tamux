export type ProviderFinalResultPresentation = {
    label: string;
    prettyJson: string;
};

export function buildProviderFinalResultPresentation(
    value: unknown,
): ProviderFinalResultPresentation | null {
    if (!value || typeof value !== "object") {
        return null;
    }

    const record = value as Record<string, unknown>;
    const label = typeof record.provider === "string" && record.provider.trim()
        ? record.provider.replace(/_/g, " ")
        : "provider result";

    try {
        return {
            label,
            prettyJson: JSON.stringify(value, null, 2),
        };
    } catch {
        return null;
    }
}