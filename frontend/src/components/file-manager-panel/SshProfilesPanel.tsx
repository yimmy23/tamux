import type { SshProfile } from "../../lib/fileManagerStore";
import { Badge, Button, Input, cn, panelSurfaceClassName } from "../ui";

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
    <div className="grid h-[16.25rem] min-h-0 grid-cols-[15rem_minmax(0,1fr)] border-t border-[var(--border-subtle)] bg-[var(--panel)]/35">
      <div className="flex min-h-0 flex-col border-r border-[var(--border-subtle)]">
        <div className="flex items-center justify-between gap-[var(--space-2)] px-[var(--space-3)] py-[var(--space-3)]">
          <div className="flex items-center gap-[var(--space-2)]">
            <strong className="text-[var(--text-sm)] text-[var(--text-primary)]">SSH Profiles</strong>
            <Badge variant="default">{sshProfiles.length}</Badge>
          </div>
          <Button type="button" variant="secondary" size="sm" onClick={addProfile}>
            New
          </Button>
        </div>

        <div className="flex-1 overflow-y-auto px-[var(--space-3)] pb-[var(--space-3)]">
          {sshProfiles.length === 0 ? (
            <div className="text-[var(--text-xs)] text-[var(--text-muted)]">No profiles saved.</div>
          ) : null}

          {sshProfiles.map((profile) => (
            <button
              key={profile.id}
              type="button"
              onClick={() => setSelectedProfileId(profile.id)}
              className={cn(
                panelSurfaceClassName,
                "mb-[var(--space-2)] grid w-full gap-[var(--space-1)] rounded-[var(--radius-lg)] px-[var(--space-3)] py-[var(--space-3)] text-left shadow-none transition-colors hover:bg-[var(--muted)]/60",
                selectedProfileId === profile.id ? "border-[var(--accent-border)] bg-[var(--accent-soft)]/40" : "bg-[var(--card)]/70"
              )}
            >
              <div className="text-[var(--text-sm)] font-semibold text-[var(--text-primary)]">
                {profile.name || "Unnamed"}
              </div>
              <div className="text-[var(--text-xs)] text-[var(--text-secondary)]">
                {(profile.user ? `${profile.user}@` : "") + (profile.host || "no-host")}
              </div>
            </button>
          ))}
        </div>
      </div>

      <div className="min-h-0 overflow-y-auto px-[var(--space-4)] py-[var(--space-3)]">
        {selectedProfile ? (
          <div className="grid gap-[var(--space-3)]">
            <div className="grid gap-[var(--space-3)] md:grid-cols-2">
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

            <div className="flex flex-wrap gap-[var(--space-2)]">
              <Button type="button" onClick={() => void runSshProfile(selectedProfile.id)} size="sm">
                Connect in Active Pane
              </Button>
              <Button
                type="button"
                variant="secondary"
                size="sm"
                onClick={async () => {
                  const command = buildSshCommand(selectedProfile.id);
                  if (!command) return;
                  await navigator.clipboard.writeText(command);
                  setStatusMessage("SSH command copied to clipboard.");
                }}
              >
                Copy Command
              </Button>
              <Button
                type="button"
                variant="destructive"
                size="sm"
                onClick={() => {
                  removeSshProfile(selectedProfile.id);
                  setStatusMessage("SSH profile removed.");
                }}
              >
                Remove Profile
              </Button>
            </div>
          </div>
        ) : (
          <div className="text-[var(--text-sm)] text-[var(--text-muted)]">
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
    <label className="grid gap-[var(--space-2)]">
      <span className="text-[var(--text-xs)] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
        {label}
      </span>
      <Input value={value} onChange={(event) => onChange(event.target.value)} />
    </label>
  );
}
