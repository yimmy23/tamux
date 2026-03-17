declare global {
    type AmuxCodingAgentCheck = {
        label: string;
        path: string;
        exists: boolean;
    };

    type AmuxCodingAgentDiscoveryResult = {
        id: string;
        available: boolean;
        executable: string | null;
        path: string | null;
        version: string | null;
        error?: string | null;
        readiness?: "ready" | "needs-setup" | "missing";
        checks?: AmuxCodingAgentCheck[];
        runtimeNotes?: string[];
        gatewayLabel?: string | null;
        gatewayReachable?: boolean | null;
    };

    type AmuxAITrainingCheck = {
        label: string;
        path: string;
        exists: boolean;
        scope: "system" | "workspace";
    };

    type AmuxAITrainingDiscoveryResult = {
        id: string;
        available: boolean;
        executable: string | null;
        path: string | null;
        version: string | null;
        readiness: "ready" | "needs-setup" | "missing";
        checks: AmuxAITrainingCheck[];
        runtimeNotes?: string[];
        workspacePath?: string | null;
        error?: string | null;
    };

    type AmuxSetupDependency = {
        name: string;
        label: string;
        command: string;
        found: boolean;
        path: string | null;
        installHints: string[];
    };

    type AmuxSetupPrereqReport = {
        profile: "source" | "desktop";
        platform: string;
        required: AmuxSetupDependency[];
        optional: AmuxSetupDependency[];
        missingRequired: string[];
        daemonPath: string;
        cliPath: string;
        installRoot: string;
        dataDir: string;
        gettingStartedPath: string;
        whatIsTamux: string;
    };

    type AmuxInstalledPluginRecord = {
        packageName: string;
        packageVersion: string;
        pluginName: string;
        entryPath: string;
        format: string;
        installedAt: number;
    };

    type AmuxInstalledPluginLoadResult = {
        packageName: string;
        pluginName: string;
        status: "loaded" | "already-loaded" | "skipped" | "error";
        error?: string;
    };

    type AmuxBridge = {
        checkSetupPrereqs?: (profile?: "source" | "desktop") => Promise<AmuxSetupPrereqReport>;
        discoverCodingAgents?: () => Promise<AmuxCodingAgentDiscoveryResult[]>;
        discoverAITraining?: (workspacePath?: string | null) => Promise<AmuxAITrainingDiscoveryResult[]>;
        getDataDir?: () => Promise<string>;
        readJsonFile?: (relativePath: string) => Promise<unknown>;
        writeJsonFile?: (relativePath: string, data: unknown) => Promise<boolean>;
        readTextFile?: (relativePath: string) => Promise<string | null>;
        writeTextFile?: (relativePath: string, content: string) => Promise<boolean>;
        deleteDataPath?: (relativePath: string) => Promise<boolean>;
        listDataDir?: (relativeDir?: string) => Promise<Array<{ name: string; path: string; isDirectory: boolean }>>;
        openDataPath?: (relativePath: string) => Promise<string>;
        revealDataPath?: (relativePath: string) => Promise<boolean>;
        dbAppendCommandLog?: (entry: unknown) => Promise<boolean>;
        dbCompleteCommandLog?: (id: string, exitCode?: number | null, durationMs?: number | null) => Promise<boolean>;
        dbQueryCommandLog?: (opts?: { workspaceId?: string | null; paneId?: string | null; limit?: number | null }) => Promise<unknown[]>;
        dbClearCommandLog?: () => Promise<boolean>;
        dbCreateThread?: (thread: unknown) => Promise<boolean>;
        dbDeleteThread?: (id: string) => Promise<boolean>;
        dbListThreads?: () => Promise<unknown[]>;
        dbGetThread?: (id: string) => Promise<{ thread: unknown; messages: unknown[] }>;
        dbAddMessage?: (message: unknown) => Promise<boolean>;
        dbListMessages?: (threadId: string, limit?: number | null) => Promise<unknown[]>;
        agentAddTask?: (payload: { title: string; description: string; priority?: string; command?: string | null; sessionId?: string | null; dependencies?: string[] }) => Promise<unknown>;
        dbUpsertTranscriptIndex?: (entry: unknown) => Promise<boolean>;
        dbListTranscriptIndex?: (workspaceId?: string | null) => Promise<unknown[]>;
        dbUpsertSnapshotIndex?: (entry: unknown) => Promise<boolean>;
        dbListSnapshotIndex?: (workspaceId?: string | null) => Promise<unknown[]>;
        dbUpsertAgentEvent?: (eventRow: unknown) => Promise<boolean>;
        dbListAgentEvents?: (opts?: { category?: string | null; paneId?: string | null; limit?: number | null }) => Promise<unknown[]>;
        sendTerminalInput?: (paneId: string | null, data: string) => Promise<boolean>;
        cloneTerminalSession?: (payload: {
            sourcePaneId?: string;
            sourceSessionId?: string | null;
            workspaceId?: string | null;
            cols?: number;
            rows?: number;
        }) => Promise<{ sessionId: string }>;
        listInstalledPlugins?: () => Promise<AmuxInstalledPluginRecord[]>;
        loadInstalledPlugins?: () => Promise<AmuxInstalledPluginLoadResult[]>;
    };

    interface Window {
        tamux?: AmuxBridge;
        amux?: AmuxBridge;
    }
}

export { };
