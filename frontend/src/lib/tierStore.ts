import { create } from "zustand";
import { getBridge } from "./bridge";
import { getDaemonAgentConfig } from "./daemonConfig";

export type CapabilityTier = "newcomer" | "familiar" | "power_user" | "expert";

const TIER_ORDER: Record<CapabilityTier, number> = {
    newcomer: 0,
    familiar: 1,
    power_user: 2,
    expert: 3,
};

export interface TierFeatureFlags {
    showGoalRuns: boolean;
    showTaskQueue: boolean;
    showGatewayConfig: boolean;
    showSubagents: boolean;
    showAdvancedSettings: boolean;
    showMemoryControls: boolean;
}

function computeFeatureFlags(tier: CapabilityTier): TierFeatureFlags {
    const ord = TIER_ORDER[tier];
    return {
        showGoalRuns: ord >= 1,
        showTaskQueue: ord >= 1,
        showGatewayConfig: ord >= 1,
        showSubagents: ord >= 2,
        showAdvancedSettings: ord >= 2,
        showMemoryControls: ord >= 3,
    };
}

interface TierState {
    currentTier: CapabilityTier;
    features: TierFeatureFlags;
    tierOrdinal: number;
    setTier: (tier: CapabilityTier) => void;
}

export const useTierStore = create<TierState>((set) => ({
    currentTier: "newcomer",
    features: computeFeatureFlags("newcomer"),
    tierOrdinal: 0,
    setTier: (tier: CapabilityTier) =>
        set({
            currentTier: tier,
            features: computeFeatureFlags(tier),
            tierOrdinal: TIER_ORDER[tier],
        }),
}));

/** Hydrate tier from daemon config on startup. */
export async function hydrateTierStore(): Promise<void> {
    const bridge = getBridge();
    if (!bridge) return;
    try {
        const config = (await getDaemonAgentConfig()) as Record<string, unknown> | undefined;
        if (!config) return;
        const tierConfig = config.tier as Record<string, unknown> | undefined;
        if (!tierConfig) return;
        // Use override > self_assessment > default
        const tierStr =
            (tierConfig.user_override as string | undefined) ??
            (tierConfig.user_self_assessment as string | undefined) ??
            "newcomer";
        const validTiers: CapabilityTier[] = ["newcomer", "familiar", "power_user", "expert"];
        const tier = validTiers.includes(tierStr as CapabilityTier)
            ? (tierStr as CapabilityTier)
            : "newcomer";
        useTierStore.getState().setTier(tier);
    } catch (e) {
        console.warn("[tier] hydration failed:", e);
    }
}

/** Check if current tier meets or exceeds the required tier. */
export function tierMeetsRequirement(required: CapabilityTier): boolean {
    return useTierStore.getState().tierOrdinal >= TIER_ORDER[required];
}

export { TIER_ORDER };
