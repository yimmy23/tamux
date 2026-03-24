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

    type AmuxGoalRunStatus =
        | "queued"
        | "planning"
        | "running"
        | "awaiting_approval"
        | "paused"
        | "completed"
        | "failed"
        | "cancelled";

    type AmuxGoalRunControlAction = "pause" | "resume" | "cancel" | "retry_step" | "rerun_from_step";

    type AmuxTodoItem = {
        id: string;
        content: string;
        status: "pending" | "in_progress" | "completed" | "blocked";
        position: number;
        step_index?: number | null;
        created_at?: number | null;
        updated_at?: number | null;
    };

    type AmuxWorkContextEntry = {
        path: string;
        previous_path?: string | null;
        kind?: "repo_change" | "artifact" | "generated_skill" | null;
        source: string;
        change_kind?: string | null;
        repo_root?: string | null;
        goal_run_id?: string | null;
        step_index?: number | null;
        session_id?: string | null;
        is_text?: boolean;
        updated_at: number;
    };

    type AmuxThreadWorkContext = {
        thread_id: string;
        entries: AmuxWorkContextEntry[];
    };

    type AmuxGoalRunStep = {
        id: string;
        title: string;
        kind: string;
        status?: string | null;
        success_condition?: string | null;
        session_id?: string | null;
    };

    type AmuxGoalRun = {
        id: string;
        title: string;
        goal: string;
        client_request_id?: string | null;
        status: AmuxGoalRunStatus;
        priority?: string | null;
        created_at: number;
        started_at?: number | null;
        completed_at?: number | null;
        thread_id?: string | null;
        current_step_index?: number | null;
        current_step_title?: string | null;
        current_step_kind?: string | null;
        replan_count?: number | null;
        plan_summary?: string | null;
        reflection_summary?: string | null;
        result?: string | null;
        error?: string | null;
        last_error?: string | null;
        failure_cause?: string | null;
        memory_updates?: string[];
        generated_skill_path?: string | null;
        child_task_ids?: string[];
        child_task_count?: number | null;
        approval_count?: number | null;
        duration_ms?: number | null;
        session_id?: string | null;
        awaiting_approval_id?: string | null;
        active_task_id?: string | null;
        steps?: AmuxGoalRunStep[];
    };

    type AmuxAgentRunStatus =
        | "queued"
        | "in_progress"
        | "awaiting_approval"
        | "blocked"
        | "failed_analyzing"
        | "completed"
        | "failed"
        | "cancelled";

    type AmuxAgentRunPriority = "low" | "normal" | "high" | "urgent";

    type AmuxAgentRun = {
        id: string;
        task_id: string;
        kind: "task" | "subagent";
        classification: "coding" | "research" | "ops" | "browser" | "messaging" | "mixed" | string;
        title: string;
        description: string;
        status: AmuxAgentRunStatus;
        priority: AmuxAgentRunPriority;
        progress: number;
        created_at: number;
        started_at?: number | null;
        completed_at?: number | null;
        thread_id?: string | null;
        session_id?: string | null;
        workspace_id?: string | null;
        source: string;
        runtime?: string | null;
        goal_run_id?: string | null;
        goal_run_title?: string | null;
        goal_step_id?: string | null;
        goal_step_title?: string | null;
        parent_run_id?: string | null;
        parent_task_id?: string | null;
        parent_thread_id?: string | null;
        parent_title?: string | null;
        blocked_reason?: string | null;
        error?: string | null;
        result?: string | null;
        last_error?: string | null;
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
        listFsDir?: (targetDir: string) => Promise<Array<{ name: string; path: string; isDirectory: boolean; sizeBytes?: number | null; modifiedAt?: number | null }>>;
        copyFsPath?: (sourcePath: string, destinationPath: string) => Promise<boolean>;
        moveFsPath?: (sourcePath: string, destinationPath: string) => Promise<boolean>;
        deleteFsPath?: (targetPath: string) => Promise<boolean>;
        createFsDirectory?: (targetDirPath: string) => Promise<boolean>;
        openFsPath?: (targetPath: string) => Promise<string>;
        revealFsPath?: (targetPath: string) => Promise<boolean>;
        readFsText?: (targetPath: string) => Promise<string | null>;
        writeFsText?: (targetPath: string, content: string) => Promise<boolean>;
        getFsPathInfo?: (targetPath: string) => Promise<{ path: string; isDirectory: boolean; sizeBytes: number; modifiedAt: number; createdAt: number } | null>;
        gitStatus?: (targetPath: string) => Promise<string>;
        gitDiff?: (targetPath: string, filePath?: string | null) => Promise<string>;
        dbAppendCommandLog?: (entry: unknown) => Promise<boolean>;
        dbCompleteCommandLog?: (id: string, exitCode?: number | null, durationMs?: number | null) => Promise<boolean>;
        dbQueryCommandLog?: (opts?: { workspaceId?: string | null; paneId?: string | null; limit?: number | null }) => Promise<unknown[]>;
        dbClearCommandLog?: () => Promise<boolean>;
        dbCreateThread?: (thread: unknown) => Promise<boolean>;
        dbDeleteThread?: (id: string) => Promise<boolean>;
        dbListThreads?: () => Promise<unknown[]>;
        dbGetThread?: (id: string) => Promise<{ thread: unknown; messages: unknown[] }>;
        dbAddMessage?: (message: unknown) => Promise<boolean>;
        dbDeleteMessage?: (threadId: string, messageId: string) => Promise<boolean>;
        dbListMessages?: (threadId: string, limit?: number | null) => Promise<unknown[]>;
        agentAddTask?: (payload: { title: string; description: string; priority?: string; command?: string | null; sessionId?: string | null; scheduledAt?: number | null; dependencies?: string[] }) => Promise<unknown>;
        agentListRuns?: () => Promise<AmuxAgentRun[] | unknown>;
        agentGetRun?: (runId: string) => Promise<AmuxAgentRun | null | unknown>;
        agentListTodos?: () => Promise<Record<string, AmuxTodoItem[]> | unknown>;
        agentGetTodos?: (threadId: string) => Promise<{ thread_id: string; items: AmuxTodoItem[] } | AmuxTodoItem[] | unknown>;
        agentGetWorkContext?: (threadId: string) => Promise<{ thread_id: string; context: AmuxThreadWorkContext } | AmuxThreadWorkContext | null | unknown>;
        agentGetGitDiff?: (repoPath: string, filePath?: string | null) => Promise<{ repo_path: string; file_path?: string | null; diff: string } | string | unknown>;
        agentGetFilePreview?: (path: string, maxBytes?: number | null) => Promise<{ path: string; content: string; truncated: boolean; is_text: boolean } | null | unknown>;
        agentStartGoalRun?: (payload: { goal: string; title?: string | null; sessionId?: string | null; priority?: string | null; threadId?: string | null; clientRequestId?: string | null }) => Promise<AmuxGoalRun | unknown>;
        agentListGoalRuns?: () => Promise<AmuxGoalRun[] | unknown>;
        agentGetGoalRun?: (goalRunId: string) => Promise<AmuxGoalRun | unknown>;
        agentControlGoalRun?: (goalRunId: string, action: AmuxGoalRunControlAction, stepIndex?: number | null) => Promise<boolean | { ok?: boolean; success?: boolean } | unknown>;
        dbUpsertTranscriptIndex?: (entry: unknown) => Promise<boolean>;
        dbListTranscriptIndex?: (workspaceId?: string | null) => Promise<unknown[]>;
        dbUpsertSnapshotIndex?: (entry: unknown) => Promise<boolean>;
        dbListSnapshotIndex?: (workspaceId?: string | null) => Promise<unknown[]>;
        dbUpsertAgentEvent?: (eventRow: unknown) => Promise<boolean>;
        dbListAgentEvents?: (opts?: { category?: string | null; paneId?: string | null; limit?: number | null }) => Promise<unknown[]>;
        startTerminalSession?: (options: {
            paneId: string;
            sessionId?: string | null;
            shell?: string | null;
            cwd?: string | null;
            workspaceId?: string | null;
            cols?: number;
            rows?: number;
            sourcePaneId?: string | null;
        }) => Promise<{ sessionId?: string | null; initialOutput?: string[]; state?: string }>;
        onTerminalEvent?: (cb: (event: any) => void) => (() => void) | void;
        sendTerminalInput?: (paneId: string | null, data: string) => Promise<boolean>;
        cloneTerminalSession?: (payload: {
            sourcePaneId?: string;
            sourceSessionId?: string | null;
            workspaceId?: string | null;
            cols?: number;
            rows?: number;
        }) => Promise<{ sessionId: string }>;
        stopTerminalSession?: (paneId: string, killSession?: boolean) => Promise<boolean>;
        executeManagedCommand?: (paneId: string, payload: unknown) => Promise<boolean>;
        agentSendMessage?: (threadId: string | null, content: string, sessionId?: string | null, contextMessages?: unknown[]) => Promise<{ ok?: boolean; error?: string } | unknown>;
        agentListThreads?: () => Promise<unknown[]>;
        agentGetThread?: (threadId: string) => Promise<unknown | null>;
        openAICodexAuthStatus?: (options?: { refresh?: boolean }) => Promise<{
            available: boolean;
            authMode?: string;
            accountId?: string;
            expiresAt?: number;
            source?: string;
            api_key?: string;
            error?: string;
        }>;
        openAICodexAuthLogin?: () => Promise<{
            available: boolean;
            authMode?: string;
            accountId?: string;
            expiresAt?: number;
            source?: string;
            api_key?: string;
            error?: string;
        }>;
        openAICodexAuthLogout?: () => Promise<{ ok: boolean }>;
        agentFetchModels?: (providerId: string, base_url: string, api_key: string) => Promise<{ models?: Array<{ id: string; name?: string; context_window?: number }>; error?: string } | unknown>;
        agentGetProviderAuthStates?: () => Promise<unknown[]>;
        agentLoginProvider?: (providerId: string, api_key: string, base_url?: string) => Promise<unknown[] | { error?: string }>;
        agentLogoutProvider?: (providerId: string) => Promise<unknown[] | { error?: string }>;
        agentValidateProvider?: (providerId: string, base_url: string, api_key: string, auth_source: string) => Promise<{ valid: boolean; error?: string; models?: unknown[] }>;
        agentGetConfig?: () => Promise<unknown>;
        agentSetConfigItem?: (keyPath: string, value: unknown) => Promise<unknown>;
        agentSetSubAgent?: (subAgentJson: string) => Promise<{ ok?: boolean; error?: string }>;
        agentRemoveSubAgent?: (subAgentId: string) => Promise<{ ok?: boolean }>;
        agentListSubAgents?: () => Promise<unknown[]>;
        agentGetConciergeConfig?: () => Promise<unknown>;
        agentSetConciergeConfig?: (config: unknown) => Promise<unknown>;
        agentDismissConciergeWelcome?: () => Promise<{ ok?: boolean }>;
        agentRequestConciergeWelcome?: () => Promise<{ ok?: boolean }>;
        listInstalledPlugins?: () => Promise<AmuxInstalledPluginRecord[]>;
        loadInstalledPlugins?: () => Promise<AmuxInstalledPluginLoadResult[]>;
    };

    interface Window {
        tamux?: AmuxBridge;
        amux?: AmuxBridge;
    }
}

export { };
