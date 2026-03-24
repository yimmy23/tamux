import type { SshProfile } from "../../lib/fileManagerStore";
import { actionButtonStyle, inputStyle, secondaryButtonStyle } from "./shared";

export function SshProfilesPanel({
    sshProfiles,
    selectedProfileId,
    selectedProfile,
    setSelectedProfileId,
    addProfile,
    updateProfile,
    buildSshCommand,
    runSshProfile,
    removeSshProfile,
    setStatusMessage,
}: {
    sshProfiles: SshProfile[];
    selectedProfileId: string | null;
    selectedProfile: SshProfile | null;
    setSelectedProfileId: (id: string | null) => void;
    addProfile: () => void;
    updateProfile: <K extends keyof SshProfile>(key: K, value: SshProfile[K]) => void;
    buildSshCommand: (id: string) => string | null;
    runSshProfile: (profileId: string) => Promise<void>;
    removeSshProfile: (id: string) => void;
    setStatusMessage: (message: string) => void;
}) {
    return (
        <div
            style={{
                height: 260,
                borderTop: "1px solid var(--border)",
                background: "var(--bg-secondary)",
                display: "grid",
                gridTemplateColumns: "240px 1fr",
                minHeight: 0,
            }}
        >
            <div style={{ borderRight: "1px solid var(--border)", display: "flex", flexDirection: "column", minHeight: 0 }}>
                <div style={{ padding: "var(--space-2)", display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                    <strong style={{ fontSize: "var(--text-sm)" }}>SSH Profiles</strong>
                    <button type="button" style={secondaryButtonStyle} onClick={addProfile}>New</button>
                </div>

                <div style={{ flex: 1, overflowY: "auto", padding: "0 var(--space-2) var(--space-2)" }}>
                    {sshProfiles.length === 0 ? (
                        <div style={{ color: "var(--text-muted)", fontSize: "var(--text-xs)" }}>No profiles saved.</div>
                    ) : null}

                    {sshProfiles.map((profile) => (
                        <button
                            key={profile.id}
                            type="button"
                            onClick={() => setSelectedProfileId(profile.id)}
                            style={{
                                width: "100%",
                                textAlign: "left",
                                marginBottom: 6,
                                padding: "8px",
                                borderRadius: "var(--radius-md)",
                                border: "1px solid",
                                borderColor: selectedProfileId === profile.id ? "var(--accent)" : "var(--border)",
                                background: selectedProfileId === profile.id ? "var(--accent-soft)" : "var(--bg-tertiary)",
                                color: "var(--text-primary)",
                                cursor: "pointer",
                            }}
                        >
                            <div style={{ fontSize: "var(--text-sm)", fontWeight: 600 }}>{profile.name || "Unnamed"}</div>
                            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>
                                {(profile.user ? `${profile.user}@` : "") + (profile.host || "no-host")}
                            </div>
                        </button>
                    ))}
                </div>
            </div>

            <div style={{ padding: "var(--space-2)", overflowY: "auto", minHeight: 0 }}>
                {selectedProfile ? (
                    <div style={{ display: "grid", gap: "var(--space-2)" }}>
                        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "var(--space-2)" }}>
                            <LabeledInput label="Name" value={selectedProfile.name} onChange={(value) => updateProfile("name", value)} />
                            <LabeledInput label="Host" value={selectedProfile.host} onChange={(value) => updateProfile("host", value)} />
                            <LabeledInput label="User" value={selectedProfile.user} onChange={(value) => updateProfile("user", value)} />
                            <LabeledInput
                                label="Port"
                                value={String(selectedProfile.port || 22)}
                                onChange={(value) => updateProfile("port", Number(value) || 22)}
                            />
                            <LabeledInput label="Key Path" value={selectedProfile.keyPath} onChange={(value) => updateProfile("keyPath", value)} />
                            <LabeledInput label="Remote Path" value={selectedProfile.remotePath} onChange={(value) => updateProfile("remotePath", value)} />
                            <LabeledInput label="Jump Host" value={selectedProfile.jumpHost} onChange={(value) => updateProfile("jumpHost", value)} />
                            <LabeledInput label="Extra Options" value={selectedProfile.options} onChange={(value) => updateProfile("options", value)} />
                        </div>

                        <div style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap" }}>
                            <button type="button" style={actionButtonStyle} onClick={() => void runSshProfile(selectedProfile.id)}>
                                Connect in Active Pane
                            </button>
                            <button
                                type="button"
                                style={secondaryButtonStyle}
                                onClick={async () => {
                                    const command = buildSshCommand(selectedProfile.id);
                                    if (!command) return;
                                    await navigator.clipboard.writeText(command);
                                    setStatusMessage("SSH command copied to clipboard.");
                                }}
                            >
                                Copy Command
                            </button>
                            <button
                                type="button"
                                style={secondaryButtonStyle}
                                onClick={() => {
                                    removeSshProfile(selectedProfile.id);
                                    setStatusMessage("SSH profile removed.");
                                }}
                            >
                                Remove Profile
                            </button>
                        </div>
                    </div>
                ) : (
                    <div style={{ color: "var(--text-muted)", fontSize: "var(--text-sm)" }}>
                        Select or create an SSH profile to edit and launch sessions.
                    </div>
                )}
            </div>
        </div>
    );
}

function LabeledInput({
    label,
    value,
    onChange,
}: {
    label: string;
    value: string;
    onChange: (value: string) => void;
}) {
    return (
        <label style={{ display: "grid", gap: 4, fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>
            <span>{label}</span>
            <input value={value} onChange={(event) => onChange(event.target.value)} style={inputStyle} />
        </label>
    );
}