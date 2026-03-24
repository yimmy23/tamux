import { Buffer as BrowserBuffer } from "buffer";
import type { AgentMessage, AgentSettings } from "./agentStore";

type HonchoPeer = {
    message: (content: string) => unknown;
    chat: (query: string) => Promise<unknown>;
};

type HonchoSession = {
    addPeers: (peers: unknown[]) => Promise<unknown>;
    addMessages: (messages: unknown[]) => Promise<unknown>;
    context: (options?: unknown) => Promise<unknown>;
};

type HonchoCtor = new (options?: {
    api_key?: string;
    base_url?: string;
    workspaceId?: string;
}) => unknown;

const syncedMessageIds = new Set<string>();
const SYNCED_IDS_MAX = 10_000;

function trackSyncedId(id: string): void {
    syncedMessageIds.add(id);
    if (syncedMessageIds.size > SYNCED_IDS_MAX) {
        const first = syncedMessageIds.values().next().value;
        if (first !== undefined) syncedMessageIds.delete(first);
    }
}
let honchoCtorPromise: Promise<HonchoCtor> | null = null;

function isEnabled(settings: AgentSettings): boolean {
    return Boolean(settings.enable_honcho_memory && settings.honcho_api_key.trim());
}

function ensureNodeLikeGlobals(): void {
    const runtime = globalThis as any;

    if (!runtime.Buffer) {
        runtime.Buffer = BrowserBuffer;
    }

    if (!runtime.process) {
        runtime.process = { env: {} };
        return;
    }

    if (!runtime.process.env) {
        runtime.process.env = {};
    }
}

async function getHonchoCtor(): Promise<HonchoCtor> {
    ensureNodeLikeGlobals();

    if (!honchoCtorPromise) {
        honchoCtorPromise = import("@honcho-ai/sdk").then(
            (module) => module.Honcho as unknown as HonchoCtor,
        );
    }

    return honchoCtorPromise;
}

async function createClient(settings: AgentSettings): Promise<any | null> {
    if (!isEnabled(settings)) return null;

    const Honcho = await getHonchoCtor();

    const options: Record<string, string> = {
        api_key: settings.honcho_api_key.trim(),
        workspaceId: settings.honcho_workspace_id.trim() || "tamux",
    };
    if (settings.honcho_base_url.trim()) {
        options.base_url = settings.honcho_base_url.trim();
    }

    return new Honcho(options);
}

async function ensureSession(settings: AgentSettings, threadId: string): Promise<{
    session: HonchoSession;
    userPeer: HonchoPeer;
    assistantPeer: HonchoPeer;
}> {
    const client = await createClient(settings);
    if (!client) {
        throw new Error("Honcho is not configured");
    }

    const userPeer = await client.peer("operator") as HonchoPeer;
    const assistantPeer = await client.peer(settings.agent_name.trim() || "assistant") as HonchoPeer;
    const session = await client.session(threadId) as HonchoSession;
    await session.addPeers([userPeer, assistantPeer]);
    return { session, userPeer, assistantPeer };
}

function toHonchoMessage(message: AgentMessage, userPeer: HonchoPeer, assistantPeer: HonchoPeer): unknown | null {
    const content = message.content.trim();

    if (message.role === "user" && content) {
        return userPeer.message(content);
    }

    if (message.role === "assistant" && content) {
        return assistantPeer.message(content);
    }

    if (message.role === "tool" && content) {
        const prefix = message.toolName ? `[tool:${message.toolName}] ` : "[tool] ";
        return assistantPeer.message(`${prefix}${content}`);
    }

    return null;
}

function normalizeText(value: unknown): string {
    if (typeof value === "string") return value.trim();
    if (!value || typeof value !== "object") return "";

    const candidate = value as Record<string, unknown>;
    const direct = [candidate.response, candidate.content, candidate.text, candidate.summary]
        .find((entry) => typeof entry === "string");
    if (typeof direct === "string") {
        return direct.trim();
    }

    if (Array.isArray(candidate.messages)) {
        const rendered = candidate.messages
            .map((message) => {
                if (!message || typeof message !== "object") return "";
                const row = message as Record<string, unknown>;
                const role = typeof row.role === "string" ? row.role : "memory";
                const text = typeof row.content === "string" ? row.content : "";
                return text ? `${role}: ${text}` : "";
            })
            .filter(Boolean)
            .join("\n");
        if (rendered) return rendered;
    }

    try {
        return JSON.stringify(value, null, 2);
    } catch {
        return "";
    }
}

export async function syncMessagesToHoncho(
    settings: AgentSettings,
    threadId: string,
    messages: AgentMessage[],
): Promise<void> {
    if (!isEnabled(settings) || !threadId || messages.length === 0) return;

    try {
        const { session, userPeer, assistantPeer } = await ensureSession(settings, threadId);
        const pending = messages
            .filter((message) => !syncedMessageIds.has(message.id))
            .sort((left, right) => left.createdAt - right.createdAt)
            .map((message) => ({ id: message.id, payload: toHonchoMessage(message, userPeer, assistantPeer) }))
            .filter((entry): entry is { id: string; payload: unknown } => entry.payload !== null);

        if (pending.length === 0) return;

        await session.addMessages(pending.map((entry) => entry.payload));
        for (const entry of pending) {
            trackSyncedId(entry.id);
        }
    } catch (error) {
        console.warn("Honcho sync failed", error);
    }
}

export async function buildHonchoContext(
    settings: AgentSettings,
    threadId: string,
    query: string,
): Promise<string> {
    if (!isEnabled(settings) || !threadId || !query.trim()) return "";

    try {
        const { session } = await ensureSession(settings, threadId);
        const context = await session.context({ query: query.trim() });
        return normalizeText(context);
    } catch (error) {
        console.warn("Honcho context lookup failed", error);
        return "";
    }
}

export async function queryHonchoMemory(
    settings: AgentSettings,
    query: string,
): Promise<string> {
    if (!isEnabled(settings) || !query.trim()) {
        return "Honcho memory is not configured.";
    }

    try {
        const client = await createClient(settings);
        if (!client) {
            return "Honcho memory is not configured.";
        }
        const assistantPeer = client.peer(settings.agent_name.trim() || "assistant") as unknown as HonchoPeer;
        const response = await assistantPeer.chat(query.trim());
        return normalizeText(response) || "No relevant memory found.";
    } catch (error: any) {
        return `Error: ${error?.message || String(error)}`;
    }
}
