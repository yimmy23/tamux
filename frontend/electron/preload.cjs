const { contextBridge, ipcRenderer } = require('electron');

const loadedExternalPluginEntries = new Set();

async function injectInstalledPluginScript(entry) {
    const normalizedEntryPath = String(entry.sourceUrl || entry.entryPath || '');
    if (!normalizedEntryPath) {
        return {
            packageName: entry.packageName,
            pluginName: entry.pluginName,
            status: 'error',
            error: 'Plugin entry path is missing.',
        };
    }

    if (loadedExternalPluginEntries.has(normalizedEntryPath)) {
        return {
            packageName: entry.packageName,
            pluginName: entry.pluginName,
            status: 'already-loaded',
        };
    }

    if (typeof entry.source !== 'string' || !entry.source.trim()) {
        return {
            packageName: entry.packageName,
            pluginName: entry.pluginName,
            status: 'error',
            error: 'Plugin source payload is missing.',
        };
    }

    const script = document.createElement('script');
    script.type = 'text/javascript';
    script.dataset.zoraiExternalPlugin = entry.packageName;
    script.textContent = `${entry.source}\n//# sourceURL=${normalizedEntryPath.replace(/\\/g, '/')}`;

    try {
        (document.head || document.documentElement).appendChild(script);
        loadedExternalPluginEntries.add(normalizedEntryPath);
        return {
            packageName: entry.packageName,
            pluginName: entry.pluginName,
            status: 'loaded',
        };
    } finally {
        script.remove();
    }
}

async function loadInstalledPlugins() {
    const installed = await ipcRenderer.invoke('plugin-load-installed');
    const results = [];

    for (const entry of installed) {
        try {
            if (entry?.status === 'error') {
                results.push({
                    packageName: entry.packageName,
                    pluginName: entry.pluginName,
                    status: 'error',
                    error: entry.error || 'Plugin load failed in the main process.',
                });
                continue;
            }

            if (entry.format !== 'script') {
                results.push({
                    packageName: entry.packageName,
                    pluginName: entry.pluginName,
                    status: 'skipped',
                    error: `Unsupported plugin format '${entry.format}'`,
                });
                continue;
            }

            if (!entry.source) {
                results.push({
                    packageName: entry.packageName,
                    pluginName: entry.pluginName,
                    status: 'error',
                    error: 'Plugin source payload is missing.',
                });
                continue;
            }

            results.push(await injectInstalledPluginScript(entry));
        } catch (error) {
            results.push({
                packageName: entry.packageName,
                pluginName: entry.pluginName,
                status: 'error',
                error: error?.message || String(error),
            });
        }
    }

    return results;
}

const bridgeApi = {
    getSocketPath: () => ipcRenderer.invoke('getSocketPath'),
    checkDaemon: () => ipcRenderer.invoke('checkDaemon'),
    spawnDaemon: () => ipcRenderer.invoke('spawnDaemon'),
    getSystemFonts: () => ipcRenderer.invoke('getSystemFonts'),
    getAvailableShells: () => ipcRenderer.invoke('getAvailableShells'),
    getSystemMonitorSnapshot: (options) => ipcRenderer.invoke('system-monitor-snapshot', options),
    getDaemonPath: () => ipcRenderer.invoke('getDaemonPath'),
    getPlatform: () => ipcRenderer.invoke('getPlatform'),
    checkSetupPrereqs: (profile) => ipcRenderer.invoke('setup-check-prereqs', profile),
    discoverCodingAgents: () => ipcRenderer.invoke('coding-agents-discover'),
    discoverAITraining: (workspacePath) => ipcRenderer.invoke('ai-training-discover', workspacePath),
    listInstalledPlugins: () => ipcRenderer.invoke('plugin-list-installed'),
    loadInstalledPlugins: () => loadInstalledPlugins(),
    pluginDaemonList: () => ipcRenderer.invoke('plugin-daemon-list'),
    pluginDaemonGet: (name) => ipcRenderer.invoke('plugin-daemon-get', name),
    pluginDaemonEnable: (name) => ipcRenderer.invoke('plugin-daemon-enable', name),
    pluginDaemonDisable: (name) => ipcRenderer.invoke('plugin-daemon-disable', name),
    pluginDaemonInstall: (dirName, installSource) => ipcRenderer.invoke('plugin-daemon-install', dirName, installSource),
    pluginDaemonUninstall: (name) => ipcRenderer.invoke('plugin-daemon-uninstall', name),
    pluginGetSettings: (name) => ipcRenderer.invoke('plugin-get-settings', name),
    pluginUpdateSettings: (pluginName, key, value, isSecret) => ipcRenderer.invoke('plugin-update-settings', pluginName, key, value, isSecret),
    pluginTestConnection: (name) => ipcRenderer.invoke('plugin-test-connection', name),
    pluginOAuthStart: (name) => ipcRenderer.invoke('plugin-oauth-start', name),
    onPluginOAuthComplete: (callback) => {
        const handler = (_event, data) => callback(data);
        ipcRenderer.on('plugin-oauth-complete', handler);
        return () => ipcRenderer.removeListener('plugin-oauth-complete', handler);
    },
    checkLspHealth: () => ipcRenderer.invoke('diagnostics-check-lsp'),
    checkMcpHealth: (servers) => ipcRenderer.invoke('diagnostics-check-mcp', servers),
    getDataDir: () => ipcRenderer.invoke('persistence-get-data-dir'),
    readJsonFile: (relativePath) => ipcRenderer.invoke('persistence-read-json', relativePath),
    writeJsonFile: (relativePath, data) => ipcRenderer.invoke('persistence-write-json', relativePath, data),
    readTextFile: (relativePath) => ipcRenderer.invoke('persistence-read-text', relativePath),
    writeTextFile: (relativePath, content) => ipcRenderer.invoke('persistence-write-text', relativePath, content),
    deleteDataPath: (relativePath) => ipcRenderer.invoke('persistence-delete-path', relativePath),
    listDataDir: (relativeDir) => ipcRenderer.invoke('persistence-list-dir', relativeDir),
    openDataPath: (relativePath) => ipcRenderer.invoke('persistence-open-path', relativePath),
    revealDataPath: (relativePath) => ipcRenderer.invoke('persistence-reveal-path', relativePath),
    listFsDir: (targetDir) => ipcRenderer.invoke('fs-list-dir', targetDir),
    copyFsPath: (sourcePath, destinationPath) => ipcRenderer.invoke('fs-copy-path', sourcePath, destinationPath),
    moveFsPath: (sourcePath, destinationPath) => ipcRenderer.invoke('fs-move-path', sourcePath, destinationPath),
    deleteFsPath: (targetPath) => ipcRenderer.invoke('fs-delete-path', targetPath),
    createFsDirectory: (targetDirPath) => ipcRenderer.invoke('fs-mkdir', targetDirPath),
    openFsPath: (targetPath) => ipcRenderer.invoke('fs-open-path', targetPath),
    revealFsPath: (targetPath) => ipcRenderer.invoke('fs-reveal-path', targetPath),
    readFsText: (targetPath) => ipcRenderer.invoke('fs-read-text', targetPath),
    writeFsText: (targetPath, content) => ipcRenderer.invoke('fs-write-text', targetPath, content),
    getFsPathInfo: (targetPath) => ipcRenderer.invoke('fs-path-info', targetPath),
    gitStatus: (targetPath) => ipcRenderer.invoke('git-status', targetPath),
    gitDiff: (targetPath, filePath) => ipcRenderer.invoke('git-diff', targetPath, filePath),
    readClipboardText: () => ipcRenderer.invoke('clipboard-read-text'),
    writeClipboardText: (text) => ipcRenderer.invoke('clipboard-write-text', text),
    startTerminalSession: (options) => ipcRenderer.invoke('terminal-start', options),
    sendTerminalInput: (paneId, data) => ipcRenderer.invoke('terminal-input', paneId, data),
    executeManagedCommand: (paneId, payload) => ipcRenderer.invoke('terminal-execute-managed', paneId, payload),
    resolveManagedApproval: (paneId, approvalId, decision) => ipcRenderer.invoke('terminal-approval-decision', paneId, approvalId, decision),
    searchManagedHistory: (paneId, query, limit) => ipcRenderer.invoke('terminal-search-history', paneId, query, limit),
    generateManagedSkill: (paneId, query, title) => ipcRenderer.invoke('terminal-generate-skill', paneId, query, title),
    findManagedSymbol: (paneId, workspaceRoot, symbol, limit) => ipcRenderer.invoke('terminal-find-symbol', paneId, workspaceRoot, symbol, limit),
    listSnapshots: (paneId, workspaceId) => ipcRenderer.invoke('terminal-list-snapshots', paneId, workspaceId),
    restoreSnapshot: (paneId, snapshotId) => ipcRenderer.invoke('terminal-restore-snapshot', paneId, snapshotId),
    cloneTerminalSession: (payload) => ipcRenderer.invoke('terminal-clone-session', payload),
    resizeTerminalSession: (paneId, cols, rows) => ipcRenderer.invoke('terminal-resize', paneId, cols, rows),
    stopTerminalSession: (paneId, killSession) => ipcRenderer.invoke('terminal-stop', paneId, killSession),
    dbAppendCommandLog: (entry) => ipcRenderer.invoke('db-append-command-log', entry),
    dbCompleteCommandLog: (id, exitCode, durationMs) => ipcRenderer.invoke('db-complete-command-log', id, exitCode, durationMs),
    dbQueryCommandLog: (opts) => ipcRenderer.invoke('db-query-command-log', opts),
    dbClearCommandLog: () => ipcRenderer.invoke('db-clear-command-log'),
    dbCreateThread: (thread) => ipcRenderer.invoke('db-create-thread', thread),
    dbDeleteThread: (id) => ipcRenderer.invoke('db-delete-thread', id),
    dbListThreads: () => ipcRenderer.invoke('db-list-threads'),
    dbGetThread: (id) => ipcRenderer.invoke('db-get-thread', id),
    dbAddMessage: (message) => ipcRenderer.invoke('db-add-message', message),
    dbDeleteMessage: (threadId, messageId) => ipcRenderer.invoke('db-delete-message', threadId, messageId),
    dbListMessages: (threadId, limit) => ipcRenderer.invoke('db-list-messages', threadId, limit),
    dbUpsertTranscriptIndex: (entry) => ipcRenderer.invoke('db-upsert-transcript-index', entry),
    dbListTranscriptIndex: (workspaceId) => ipcRenderer.invoke('db-list-transcript-index', workspaceId),
    dbUpsertSnapshotIndex: (entry) => ipcRenderer.invoke('db-upsert-snapshot-index', entry),
    dbListSnapshotIndex: (workspaceId) => ipcRenderer.invoke('db-list-snapshot-index', workspaceId),
    dbUpsertAgentEvent: (eventRow) => ipcRenderer.invoke('db-upsert-agent-event', eventRow),
    dbListAgentEvents: (opts) => ipcRenderer.invoke('db-list-agent-events', opts),
    dbListDatabaseTables: () => ipcRenderer.invoke('db-list-database-tables'),
    dbQueryDatabaseRows: (opts) => ipcRenderer.invoke('db-query-database-rows', opts),
    dbUpdateDatabaseRows: (tableName, updates) => ipcRenderer.invoke('db-update-database-rows', tableName, updates),
    dbQueueSemanticBackfill: (limit) => ipcRenderer.invoke('db-queue-semantic-backfill', limit),
    dbGetSemanticIndexStatus: (opts) => ipcRenderer.invoke('db-get-semantic-index-status', opts),
    onTerminalEvent: (cb) => {
        const listener = (_event, payload) => cb(payload);
        ipcRenderer.on('terminal-event', listener);
        return () => ipcRenderer.removeListener('terminal-event', listener);
    },
    onAppCommand: (cb) => {
        const listener = (_event, command) => cb(command);
        ipcRenderer.on('app-command', listener);
        return () => ipcRenderer.removeListener('app-command', listener);
    },
    windowMinimize: () => ipcRenderer.invoke('window-minimize'),
    windowMaximize: () => ipcRenderer.invoke('window-maximize'),
    windowClose: () => ipcRenderer.invoke('window-close'),
    windowIsMaximized: () => ipcRenderer.invoke('window-isMaximized'),
    setWindowOpacity: (opacity) => ipcRenderer.invoke('window-set-opacity', opacity),
    saveVisionScreenshot: (payload) => ipcRenderer.invoke('vision-save-screenshot', payload),
    onWindowState: (cb) => {
        ipcRenderer.on('window-state', (_e, state) => cb(state));
        return () => ipcRenderer.removeAllListeners('window-state');
    },
    sendDiscordMessage: (payload) => ipcRenderer.invoke('discord-send-message', payload),

    // WhatsApp link bridge (daemon protocol by default, Electron sidecar via fallback flag)
    whatsappConnect: () => ipcRenderer.invoke('whatsapp-connect'),
    whatsappDisconnect: () => ipcRenderer.invoke('whatsapp-disconnect'),
    whatsappStatus: () => ipcRenderer.invoke('whatsapp-status'),
    whatsappSend: (jid, text) => ipcRenderer.invoke('whatsapp-send', jid, text),
    onWhatsAppQR: (cb) => {
        const listener = (_event, dataUrl) => cb(dataUrl);
        ipcRenderer.on('whatsapp-qr', listener);
        return () => ipcRenderer.removeListener('whatsapp-qr', listener);
    },
    onWhatsAppConnected: (cb) => {
        const listener = (_event, info) => cb(info);
        ipcRenderer.on('whatsapp-connected', listener);
        return () => ipcRenderer.removeListener('whatsapp-connected', listener);
    },
    onWhatsAppDisconnected: (cb) => {
        const listener = (_event, info) => cb(info);
        ipcRenderer.on('whatsapp-disconnected', listener);
        return () => ipcRenderer.removeListener('whatsapp-disconnected', listener);
    },
    onWhatsAppError: (cb) => {
        const listener = (_event, msg) => cb(msg);
        ipcRenderer.on('whatsapp-error', listener);
        return () => ipcRenderer.removeListener('whatsapp-error', listener);
    },
    onWhatsAppMessage: (cb) => {
        const listener = (_event, msg) => cb(msg);
        ipcRenderer.on('whatsapp-message', listener);
        return () => ipcRenderer.removeListener('whatsapp-message', listener);
    },

    // Agent engine (daemon-side)
    agentSendMessage: (threadId, content, sessionId, contextMessages, contentBlocksJson) => ipcRenderer.invoke('agent-send-message', threadId, content, sessionId, contextMessages, contentBlocksJson),
    agentInternalDelegate: (threadId, targetAgentId, content, sessionId) => ipcRenderer.invoke('agent-internal-delegate', threadId, targetAgentId, content, sessionId),
    agentThreadParticipantCommand: (payload) => ipcRenderer.invoke('agent-thread-participant-command', payload),
    agentSendParticipantSuggestion: (payload) => ipcRenderer.invoke('agent-send-participant-suggestion', payload),
    agentDismissParticipantSuggestion: (payload) => ipcRenderer.invoke('agent-dismiss-participant-suggestion', payload),
    agentStopStream: (threadId) => ipcRenderer.invoke('agent-stop-stream', threadId),
    agentListThreads: () => ipcRenderer.invoke('agent-list-threads'),
    agentGetThread: (threadId, options) => ipcRenderer.invoke('agent-get-thread', threadId, options),
    agentPinThreadMessageForCompaction: (threadId, messageId) => ipcRenderer.invoke('agent-pin-thread-message-for-compaction', threadId, messageId),
    agentUnpinThreadMessageForCompaction: (threadId, messageId) => ipcRenderer.invoke('agent-unpin-thread-message-for-compaction', threadId, messageId),
    agentDeleteThread: (threadId) => ipcRenderer.invoke('agent-delete-thread', threadId),
    agentAddTask: (payload) => ipcRenderer.invoke('agent-add-task', payload),
    agentCancelTask: (taskId) => ipcRenderer.invoke('agent-cancel-task', taskId),
    agentListTasks: () => ipcRenderer.invoke('agent-list-tasks'),
    agentListRuns: () => ipcRenderer.invoke('agent-list-runs'),
    agentGetRun: (runId) => ipcRenderer.invoke('agent-get-run', runId),
    agentListTodos: () => ipcRenderer.invoke('agent-list-todos'),
    agentGetTodos: (threadId) => ipcRenderer.invoke('agent-get-todos', threadId),
    agentGetWorkContext: (threadId) => ipcRenderer.invoke('agent-get-work-context', threadId),
    agentGetGitDiff: (repoPath, filePath) => ipcRenderer.invoke('agent-get-git-diff', repoPath, filePath),
    agentGetFilePreview: (filePath, maxBytes) => ipcRenderer.invoke('agent-get-file-preview', filePath, maxBytes),
    agentStartGoalRun: (payload) => ipcRenderer.invoke('agent-start-goal-run', payload),
    agentListGoalRuns: () => ipcRenderer.invoke('agent-list-goal-runs'),
    agentGetGoalRun: (goalRunId) => ipcRenderer.invoke('agent-get-goal-run', goalRunId),
    agentControlGoalRun: (goalRunId, action, stepIndex) => ipcRenderer.invoke('agent-control-goal-run', goalRunId, action, stepIndex),
    agentListWorkspaceSettings: () => ipcRenderer.invoke('agent-list-workspace-settings'),
    agentGetWorkspaceSettings: (workspaceId) => ipcRenderer.invoke('agent-get-workspace-settings', workspaceId),
    agentSetWorkspaceOperator: (workspaceId, operator) => ipcRenderer.invoke('agent-set-workspace-operator', workspaceId, operator),
    agentListWorkspaceTasks: (workspaceId, includeDeleted) => ipcRenderer.invoke('agent-list-workspace-tasks', workspaceId, includeDeleted),
    agentCreateWorkspaceTask: (request) => ipcRenderer.invoke('agent-create-workspace-task', request),
    agentUpdateWorkspaceTask: (taskId, update) => ipcRenderer.invoke('agent-update-workspace-task', taskId, update),
    agentMoveWorkspaceTask: (request) => ipcRenderer.invoke('agent-move-workspace-task', request),
    agentRunWorkspaceTask: (taskId) => ipcRenderer.invoke('agent-run-workspace-task', taskId),
    agentPauseWorkspaceTask: (taskId) => ipcRenderer.invoke('agent-pause-workspace-task', taskId),
    agentStopWorkspaceTask: (taskId) => ipcRenderer.invoke('agent-stop-workspace-task', taskId),
    agentDeleteWorkspaceTask: (taskId) => ipcRenderer.invoke('agent-delete-workspace-task', taskId),
    agentListWorkspaceNotices: (workspaceId, taskId) => ipcRenderer.invoke('agent-list-workspace-notices', workspaceId, taskId),
    agentExplainAction: (actionId, stepIndex) => ipcRenderer.invoke('agent-explain-action', actionId, stepIndex),
    agentStartDivergentSession: (payload) => ipcRenderer.invoke('agent-start-divergent-session', payload),
    agentGetDivergentSession: (sessionId) => ipcRenderer.invoke('agent-get-divergent-session', sessionId),
    agentGetConfig: () => ipcRenderer.invoke('agent-get-config'),
    agentGetStatus: () => ipcRenderer.invoke('agent-get-status'),
    agentGetStatistics: (window) => ipcRenderer.invoke('agent-get-statistics', window),
    agentInspectPrompt: (agentId) => ipcRenderer.invoke('agent-inspect-prompt', agentId),
    agentQueryAudits: (actionTypes, since, limit) => ipcRenderer.invoke('agent-query-audits', actionTypes, since, limit),
    agentGetProvenanceReport: (limit) => ipcRenderer.invoke('agent-get-provenance-report', limit),
    agentGetMemoryProvenanceReport: (target, limit) => ipcRenderer.invoke('agent-get-memory-provenance-report', target, limit),
    agentConfirmMemoryProvenanceEntry: (entryId) => ipcRenderer.invoke('agent-confirm-memory-provenance-entry', entryId),
    agentRetractMemoryProvenanceEntry: (entryId) => ipcRenderer.invoke('agent-retract-memory-provenance-entry', entryId),
    agentGetCollaborationSessions: (parentTaskId) => ipcRenderer.invoke('agent-get-collaboration-sessions', parentTaskId),
    agentVoteOnCollaborationDisagreement: (parentTaskId, disagreementId, taskId, position, confidence) => ipcRenderer.invoke('agent-vote-on-collaboration-disagreement', parentTaskId, disagreementId, taskId, position, confidence),
    agentListGeneratedTools: () => ipcRenderer.invoke('agent-list-generated-tools'),
    agentRunGeneratedTool: (toolName, argsJson) => ipcRenderer.invoke('agent-run-generated-tool', toolName, argsJson),
    agentSpeechToText: (base64Audio, mimeType, options) => ipcRenderer.invoke('agent-speech-to-text', base64Audio, mimeType, options),
    agentTextToSpeech: (text, voice, options) => ipcRenderer.invoke('agent-text-to-speech', text, voice, options),
    agentGenerateImage: (prompt, options) => ipcRenderer.invoke('agent-generate-image', prompt, options),
    agentActivateGeneratedTool: (toolName) => ipcRenderer.invoke('agent-activate-generated-tool', toolName),
    agentPromoteGeneratedTool: (toolName) => ipcRenderer.invoke('agent-promote-generated-tool', toolName),
    agentRetireGeneratedTool: (toolName) => ipcRenderer.invoke('agent-retire-generated-tool', toolName),
    agentGetOperatorModel: () => ipcRenderer.invoke('agent-get-operator-model'),
    agentResetOperatorModel: () => ipcRenderer.invoke('agent-reset-operator-model'),
    agentSetConfigItem: (keyPath, value) => ipcRenderer.invoke('agent-set-config-item', keyPath, value),
    agentSetProviderModel: (providerId, model) => ipcRenderer.invoke('agent-set-provider-model', providerId, model),
    agentSetTargetAgentProviderModel: (targetAgentId, providerId, model) => ipcRenderer.invoke('agent-set-target-agent-provider-model', targetAgentId, providerId, model),
    agentSetTierOverride: (tier) => ipcRenderer.invoke('agent-set-tier-override', tier),
    gatewayGetConfig: () => ipcRenderer.invoke('gateway:get-config'),
    gatewaySetConfig: (patch) => ipcRenderer.invoke('gateway:set-config', patch),
    openAICodexAuthStatus: (options) => ipcRenderer.invoke('openai-codex-auth-status', options),
    openAICodexAuthLogin: () => ipcRenderer.invoke('openai-codex-auth-login'),
    openAICodexAuthLogout: () => ipcRenderer.invoke('openai-codex-auth-logout'),
    agentHeartbeatGetItems: () => ipcRenderer.invoke('agent-heartbeat-get-items'),
    agentHeartbeatSetItems: (items) => ipcRenderer.invoke('agent-heartbeat-set-items', items),
    agentResolveTaskApproval: (approvalId, decision) => ipcRenderer.invoke('agent-resolve-task-approval', approvalId, decision),
    agentFetchModels: (providerId, baseUrl, apiKey) => ipcRenderer.invoke('agent-fetch-models', providerId, baseUrl, apiKey),
    agentGetProviderAuthStates: () => ipcRenderer.invoke('agent-get-provider-auth-states'),
    agentGetProviderCatalog: () => ipcRenderer.invoke('agent-get-provider-catalog'),
    agentLoginProvider: (providerId, apiKey, baseUrl) => ipcRenderer.invoke('agent-login-provider', providerId, apiKey, baseUrl),
    agentLogoutProvider: (providerId) => ipcRenderer.invoke('agent-logout-provider', providerId),
    agentValidateProvider: (providerId, baseUrl, apiKey, authSource) => ipcRenderer.invoke('agent-validate-provider', providerId, baseUrl, apiKey, authSource),
    agentSetSubAgent: (subAgentJson) => ipcRenderer.invoke('agent-set-sub-agent', subAgentJson),
    agentRemoveSubAgent: (subAgentId) => ipcRenderer.invoke('agent-remove-sub-agent', subAgentId),
    agentListSubAgents: () => ipcRenderer.invoke('agent-list-sub-agents'),
    agentGetConciergeConfig: () => ipcRenderer.invoke('agent-get-concierge-config'),
    agentSetConciergeConfig: (config) => ipcRenderer.invoke('agent-set-concierge-config', config),
    agentDismissConciergeWelcome: () => ipcRenderer.invoke('agent-dismiss-concierge-welcome'),
    agentRequestConciergeWelcome: () => ipcRenderer.invoke('agent-request-concierge-welcome'),
    dismissAuditEntry: (entryId) => ipcRenderer.invoke('dismiss-audit-entry', entryId),
    agentStartOperatorProfileSession: (kind) => ipcRenderer.invoke('agent-start-operator-profile-session', kind),
    agentNextOperatorProfileQuestion: (sessionId) => ipcRenderer.invoke('agent-next-operator-profile-question', sessionId),
    agentSubmitOperatorProfileAnswer: (sessionId, questionId, answerJson) => ipcRenderer.invoke('agent-submit-operator-profile-answer', sessionId, questionId, answerJson),
    agentSkipOperatorProfileQuestion: (sessionId, questionId, reason) => ipcRenderer.invoke('agent-skip-operator-profile-question', sessionId, questionId, reason),
    agentDeferOperatorProfileQuestion: (sessionId, questionId, deferUntilUnixMs) => ipcRenderer.invoke('agent-defer-operator-profile-question', sessionId, questionId, deferUntilUnixMs),
    agentGetOperatorProfileSummary: () => ipcRenderer.invoke('agent-get-operator-profile-summary'),
    agentSetOperatorProfileConsent: (consentKey, granted) => ipcRenderer.invoke('agent-set-operator-profile-consent', consentKey, granted),
    agentAnswerQuestion: (questionId, answer) => ipcRenderer.invoke('agent-answer-question', questionId, answer),
    onAgentEvent: (cb) => {
        const listener = (_event, data) => cb(data);
        ipcRenderer.on('agent-event', listener);
        return () => ipcRenderer.removeListener('agent-event', listener);
    },
};

contextBridge.exposeInMainWorld('zorai', bridgeApi);
