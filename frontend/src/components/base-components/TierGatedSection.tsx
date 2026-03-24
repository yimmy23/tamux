import { type ReactNode, useState } from "react";
import { useTierStore, TIER_ORDER, type CapabilityTier } from "../../lib/tierStore";

export function TierGatedSection({
    requiredTier,
    label,
    children,
}: {
    requiredTier: CapabilityTier;
    label: string;
    children: ReactNode;
}) {
    const tierOrdinal = useTierStore((s) => s.tierOrdinal);
    const requiredOrdinal = TIER_ORDER[requiredTier];
    const [expanded, setExpanded] = useState(false);

    // Tier met: render normally
    if (tierOrdinal >= requiredOrdinal) {
        return <>{children}</>;
    }

    // Tier not met: render as collapsed section (D-05)
    return (
        <div className="tier-gated-section" style={{ opacity: 0.6 }}>
            <button
                type="button"
                onClick={() => setExpanded(!expanded)}
                style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "0.5rem",
                    padding: "0.5rem",
                    background: "none",
                    border: "none",
                    cursor: "pointer",
                    color: "inherit",
                    fontSize: "0.85rem",
                    width: "100%",
                    textAlign: "left",
                }}
            >
                <span style={{ transform: expanded ? "rotate(90deg)" : "none", transition: "transform 0.15s" }}>
                    {"\u25B6"}
                </span>
                <span>{label}</span>
                <span style={{ fontSize: "0.75rem", opacity: 0.6, marginLeft: "auto" }}>
                    {requiredTier.replace("_", " ")}
                </span>
            </button>
            {expanded && <div style={{ paddingLeft: "1.5rem" }}>{children}</div>}
        </div>
    );
}
