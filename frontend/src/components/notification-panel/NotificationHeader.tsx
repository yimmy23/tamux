import { actionBtnStyle } from "./shared";

export function NotificationHeader({
    unreadCount,
    totalCount,
    markAllRead,
    clearAll,
    close,
}: {
    unreadCount: number;
    totalCount: number;
    markAllRead: () => void;
    clearAll: () => void;
    close: () => void;
}) {
    return (
        <div
            style={{
                display: "grid",
                gap: 12,
                padding: "18px 18px 14px",
                borderBottom: "1px solid rgba(255,255,255,0.08)",
            }}
        >
            <div style={{ display: "flex", alignItems: "start", justifyContent: "space-between", gap: 12 }}>
                <div style={{ display: "grid", gap: 6 }}>
                    <span className="amux-panel-title" style={{ color: "var(--mission)" }}>Mission Feed</span>
                    <span style={{ fontSize: 20, fontWeight: 800 }}>Notifications</span>
                </div>
                <div style={{ display: "flex", gap: 8 }}>
                    <button onClick={markAllRead} style={actionBtnStyle} title="Mark all read">Read</button>
                    <button onClick={clearAll} style={actionBtnStyle} title="Clear all">Purge</button>
                    <button onClick={close} style={actionBtnStyle} title="Close">✕</button>
                </div>
            </div>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(3, minmax(0, 1fr))", gap: 10 }}>
                <MetricCard label="Unread" value={String(unreadCount)} />
                <MetricCard label="Total" value={String(totalCount)} />
                <MetricCard label="State" value={unreadCount > 0 ? "attention" : "quiet"} />
            </div>
        </div>
    );
}

function MetricCard({ label, value }: { label: string; value: string }) {
    return (
        <div style={{ borderRadius: 0, padding: "10px 12px", border: "1px solid rgba(255,255,255,0.06)", background: "rgba(18, 33, 47, 0.8)", display: "grid", gap: 4 }}>
            <span className="amux-panel-title">{label}</span>
            <span style={{ fontSize: 15, fontWeight: 700 }}>{value}</span>
        </div>
    );
}
