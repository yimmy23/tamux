const { pathToFileURL } = require('node:url');

function registerAgentIpcHandlers(ipcMain, runtime, options = {}) {
    const { sendAgentCommand, sendAgentQuery } = runtime;
    const { logToFile, openAICodexAuthHandlers, saveTempAudioCapture } = options;

    ipcMain.handle('agent-send-message', async (_event, threadId, content, sessionId, contextMessages, contentBlocksJson) => {
        try {
            logToFile('info', 'agent-send-message', {
                threadId,
                contentLen: content?.length,
                sessionId,
                contextCount: Array.isArray(contextMessages) ? contextMessages.length : 0,
                contentBlocksLen: typeof contentBlocksJson === 'string' ? contentBlocksJson.length : 0,
            });
            const cmd = {
                type: 'send-message',
                thread_id: threadId || null,
                content,
                session_id: typeof sessionId === 'string' && sessionId.trim() ? sessionId.trim() : null,
            };
            if (Array.isArray(contextMessages) && contextMessages.length > 0) {
                cmd.context_messages = contextMessages;
                logToFile('info', 'agent-send-message context roles', { roles: contextMessages.map((message) => message.role) });
            }
            if (typeof contentBlocksJson === 'string' && contentBlocksJson.trim()) {
                cmd.content_blocks_json = contentBlocksJson;
            }
            sendAgentCommand(cmd);
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });
    ipcMain.handle('agent-internal-delegate', async (_event, threadId, targetAgentId, content, sessionId) => {
        try {
            sendAgentCommand({
                type: 'internal-delegate',
                thread_id: typeof threadId === 'string' && threadId.trim() ? threadId.trim() : null,
                target_agent_id: targetAgentId,
                content,
                session_id: typeof sessionId === 'string' && sessionId.trim() ? sessionId.trim() : null,
            });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });
    ipcMain.handle('agent-thread-participant-command', async (_event, payload) => {
        try {
            sendAgentCommand({
                type: 'thread-participant-command',
                thread_id: payload?.threadId,
                target_agent_id: payload?.targetAgentId,
                action: payload?.action,
                instruction: typeof payload?.instruction === 'string' && payload.instruction.trim() ? payload.instruction : null,
                session_id: typeof payload?.sessionId === 'string' && payload.sessionId.trim() ? payload.sessionId.trim() : null,
            });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });
    ipcMain.handle('agent-send-participant-suggestion', async (_event, payload) => {
        try {
            sendAgentCommand({
                type: 'send-participant-suggestion',
                thread_id: payload?.threadId,
                suggestion_id: payload?.suggestionId,
                session_id: typeof payload?.sessionId === 'string' && payload.sessionId.trim() ? payload.sessionId.trim() : null,
            });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });
    ipcMain.handle('agent-dismiss-participant-suggestion', async (_event, payload) => {
        try {
            sendAgentCommand({
                type: 'dismiss-participant-suggestion',
                thread_id: payload?.threadId,
                suggestion_id: payload?.suggestionId,
                session_id: typeof payload?.sessionId === 'string' && payload.sessionId.trim() ? payload.sessionId.trim() : null,
            });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });
    ipcMain.handle('agent-stop-stream', async (_event, threadId) => { try { sendAgentCommand({ type: 'stop-stream', thread_id: threadId }); } catch {} return { ok: true }; });
    ipcMain.handle('agent-list-threads', async () => { try { return await sendAgentQuery({ type: 'list-threads' }, 'thread-list'); } catch { return []; } });
    ipcMain.handle('agent-get-thread', async (_event, threadId, options) => {
        try {
            const messageLimit = Number.isFinite(options?.messageLimit)
                ? Number(options.messageLimit)
                : null;
            const messageOffset = Number.isFinite(options?.messageOffset)
                ? Number(options.messageOffset)
                : null;
            return await sendAgentQuery({
                type: 'get-thread',
                thread_id: threadId,
                message_limit: messageLimit,
                message_offset: messageOffset,
            }, 'thread-detail');
        } catch {
            return null;
        }
    });
    ipcMain.handle('agent-pin-thread-message-for-compaction', async (_event, threadId, messageId) => {
        try {
            return await sendAgentQuery({
                type: 'pin-thread-message-for-compaction',
                thread_id: threadId,
                message_id: messageId,
            }, 'thread-message-pin-result');
        } catch (err) {
            return { ok: false, thread_id: threadId, message_id: messageId, error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-unpin-thread-message-for-compaction', async (_event, threadId, messageId) => {
        try {
            return await sendAgentQuery({
                type: 'unpin-thread-message-for-compaction',
                thread_id: threadId,
                message_id: messageId,
            }, 'thread-message-pin-result');
        } catch (err) {
            return { ok: false, thread_id: threadId, message_id: messageId, error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-delete-thread', async (_event, threadId) => { try { sendAgentCommand({ type: 'delete-thread', thread_id: threadId }); return true; } catch { return false; } });
    ipcMain.handle('agent-add-task', async (_event, payload) => {
        try {
            sendAgentCommand({
                type: 'add-task',
                title: payload?.title,
                description: payload?.description,
                priority: payload?.priority || 'normal',
                command: typeof payload?.command === 'string' && payload.command.trim() ? payload.command.trim() : null,
                session_id: typeof payload?.sessionId === 'string' && payload.sessionId.trim() ? payload.sessionId.trim() : null,
                scheduled_at: Number.isFinite(payload?.scheduledAt) ? Number(payload.scheduledAt) : null,
                dependencies: Array.isArray(payload?.dependencies) ? payload.dependencies.filter((value) => typeof value === 'string' && value.trim()).map((value) => value.trim()) : [],
            });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });
    ipcMain.handle('agent-cancel-task', async (_event, taskId) => { try { sendAgentCommand({ type: 'cancel-task', task_id: taskId }); return true; } catch { return false; } });
    ipcMain.handle('agent-list-tasks', async () => { try { return await sendAgentQuery({ type: 'list-tasks' }, 'task-list'); } catch { return []; } });
    ipcMain.handle('agent-list-runs', async () => { try { return await sendAgentQuery({ type: 'list-runs' }, 'run-list'); } catch { return []; } });
    ipcMain.handle('agent-get-run', async (_event, runId) => { try { return await sendAgentQuery({ type: 'get-run', run_id: runId }, 'run-detail'); } catch { return null; } });
    ipcMain.handle('agent-list-todos', async () => { try { return await sendAgentQuery({ type: 'list-todos' }, 'todo-list'); } catch { return {}; } });
    ipcMain.handle('agent-get-todos', async (_event, threadId) => { try { return await sendAgentQuery({ type: 'get-todos', thread_id: threadId }, 'todo-detail'); } catch { return { thread_id: threadId, items: [] }; } });
    ipcMain.handle('agent-get-work-context', async (_event, threadId) => { try { return await sendAgentQuery({ type: 'get-work-context', thread_id: threadId }, 'work-context-detail'); } catch { return { thread_id: threadId, context: { thread_id: threadId, entries: [] } }; } });
    ipcMain.handle('agent-get-git-diff', async (_event, repoPath, filePath) => { try { return await sendAgentQuery({ type: 'get-git-diff', repo_path: repoPath, file_path: typeof filePath === 'string' && filePath.trim() ? filePath.trim() : null }, 'git-diff'); } catch { return { repo_path: repoPath, file_path: filePath ?? null, diff: '' }; } });
    ipcMain.handle('agent-get-file-preview', async (_event, filePath, maxBytes) => { try { return await sendAgentQuery({ type: 'get-file-preview', path: filePath, max_bytes: Number.isFinite(maxBytes) ? Math.max(1024, Math.trunc(maxBytes)) : null }, 'file-preview'); } catch { return { path: filePath, content: '', truncated: false, is_text: false }; } });
    ipcMain.handle('agent-start-goal-run', async (_event, payload) => { try { return await sendAgentQuery({ type: 'start-goal-run', goal: payload?.goal, title: typeof payload?.title === 'string' && payload.title.trim() ? payload.title.trim() : null, thread_id: typeof payload?.threadId === 'string' && payload.threadId.trim() ? payload.threadId.trim() : null, session_id: typeof payload?.sessionId === 'string' && payload.sessionId.trim() ? payload.sessionId.trim() : null, priority: typeof payload?.priority === 'string' && payload.priority.trim() ? payload.priority.trim() : null, client_request_id: typeof payload?.clientRequestId === 'string' && payload.clientRequestId.trim() ? payload.clientRequestId.trim() : null }, 'goal-run-started'); } catch (err) { return { ok: false, error: err?.message || String(err) }; } });
    ipcMain.handle('agent-list-goal-runs', async () => { try { return await sendAgentQuery({ type: 'list-goal-runs' }, 'goal-run-list'); } catch { return []; } });
    ipcMain.handle('agent-get-goal-run', async (_event, goalRunId) => { try { return await sendAgentQuery({ type: 'get-goal-run', goal_run_id: goalRunId }, 'goal-run-detail'); } catch { return null; } });
    ipcMain.handle('agent-control-goal-run', async (_event, goalRunId, action, stepIndex) => { try { return await sendAgentQuery({ type: 'control-goal-run', goal_run_id: goalRunId, action, step_index: Number.isFinite(stepIndex) ? Math.trunc(stepIndex) : null }, 'goal-run-controlled'); } catch { return { ok: false }; } });
    ipcMain.handle('agent-explain-action', async (_event, actionId, stepIndex) => { try { return await sendAgentQuery({ type: 'explain-action', action_id: actionId, step_index: Number.isFinite(stepIndex) ? Math.trunc(stepIndex) : null }, 'agent-explanation'); } catch (err) { return { ok: false, error: err?.message || String(err) }; } });
    ipcMain.handle('agent-start-divergent-session', async (_event, payload) => { try { return await sendAgentQuery({ type: 'start-divergent-session', problem_statement: payload?.problemStatement, thread_id: payload?.threadId, goal_run_id: typeof payload?.goalRunId === 'string' && payload.goalRunId.trim() ? payload.goalRunId.trim() : null, custom_framings_json: typeof payload?.customFramingsJson === 'string' && payload.customFramingsJson.trim() ? payload.customFramingsJson : null }, 'agent-divergent-session-started'); } catch (err) { return { ok: false, error: err?.message || String(err) }; } });
    ipcMain.handle('agent-get-divergent-session', async (_event, sessionId) => { try { return await sendAgentQuery({ type: 'get-divergent-session', session_id: sessionId }, 'agent-divergent-session'); } catch (err) { return { ok: false, error: err?.message || String(err) }; } });
    ipcMain.handle('agent-start-operator-profile-session', async (_event, kind) => { try { return await sendAgentQuery({ type: 'start-operator-profile-session', kind: kind || 'first_run_onboarding' }, 'operator-profile-session-started'); } catch (err) { return { error: err?.message || String(err) }; } });
    ipcMain.handle('agent-next-operator-profile-question', async (_event, sessionId) => { try { return await sendAgentQuery({ type: 'next-operator-profile-question', session_id: sessionId }, ['operator-profile-question', 'operator-profile-session-completed']); } catch (err) { return { error: err?.message || String(err) }; } });
    ipcMain.handle('agent-submit-operator-profile-answer', async (_event, sessionId, questionId, answerJson) => { try { return await sendAgentQuery({ type: 'submit-operator-profile-answer', session_id: sessionId, question_id: questionId, answer_json: answerJson }, ['operator-profile-progress', 'operator-profile-session-completed']); } catch (err) { return { error: err?.message || String(err) }; } });
    ipcMain.handle('agent-skip-operator-profile-question', async (_event, sessionId, questionId, reason) => { try { return await sendAgentQuery({ type: 'skip-operator-profile-question', session_id: sessionId, question_id: questionId, reason: typeof reason === 'string' ? reason : null }, ['operator-profile-progress', 'operator-profile-session-completed']); } catch (err) { return { error: err?.message || String(err) }; } });
    ipcMain.handle('agent-defer-operator-profile-question', async (_event, sessionId, questionId, deferUntilUnixMs) => { try { return await sendAgentQuery({ type: 'defer-operator-profile-question', session_id: sessionId, question_id: questionId, defer_until_unix_ms: Number.isFinite(deferUntilUnixMs) ? Math.trunc(deferUntilUnixMs) : null }, ['operator-profile-progress', 'operator-profile-session-completed']); } catch (err) { return { error: err?.message || String(err) }; } });
    ipcMain.handle('agent-get-operator-profile-summary', async () => { try { return await sendAgentQuery({ type: 'get-operator-profile-summary' }, 'operator-profile-summary'); } catch (err) { return { error: err?.message || String(err) }; } });
    ipcMain.handle('agent-set-operator-profile-consent', async (_event, consentKey, granted) => { try { await sendAgentQuery({ type: 'set-operator-profile-consent', consent_key: consentKey, granted: Boolean(granted) }, 'operator-profile-session-completed'); return { ok: true }; } catch (err) { return { error: err?.message || String(err) }; } });
    ipcMain.handle('agent-answer-question', async (_event, questionId, answer) => { try { sendAgentCommand({ type: 'answer-question', question_id: questionId, answer }); return { ok: true }; } catch (err) { return { ok: false, error: err?.message || String(err) }; } });
    ipcMain.handle('agent-get-concierge-config', async () => sendAgentQuery({ type: 'get-concierge-config' }, 'concierge-config'));
    ipcMain.handle('agent-set-concierge-config', async (_event, config) => { try { sendAgentCommand({ type: 'set-concierge-config', config_json: JSON.stringify(config) }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('agent-dismiss-concierge-welcome', async () => { try { sendAgentCommand({ type: 'dismiss-concierge-welcome' }); return { ok: true }; } catch { return { ok: false }; } });
    ipcMain.handle('agent-request-concierge-welcome', async () => { try { sendAgentCommand({ type: 'request-concierge-welcome' }); return { ok: true }; } catch { return { ok: false }; } });
    ipcMain.handle('dismiss-audit-entry', async (_event, entryId) => { try { sendAgentCommand({ type: 'audit-dismiss', entry_id: entryId }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('agent-get-config', async () => sendAgentQuery({ type: 'get-config' }, 'config'));
    ipcMain.handle('agent-get-status', async () => { try { return await sendAgentQuery({ type: 'get-status' }, 'status-response'); } catch (err) { logToFile('warn', 'agent-get-status failed', { error: err?.message ?? String(err) }); return null; } });
    ipcMain.handle('agent-get-statistics', async (_event, window) => {
        try {
            return await sendAgentQuery({
                type: 'agent-get-statistics',
                window: typeof window === 'string' && window.trim() ? window.trim() : 'all',
            }, 'statistics-response');
        } catch (err) {
            logToFile('warn', 'agent-get-statistics failed', { error: err?.message ?? String(err), window });
            return null;
        }
    });
    ipcMain.handle('agent-inspect-prompt', async (_event, agentId) => {
        try {
            return await sendAgentQuery({
                type: 'inspect-prompt',
                agent_id: typeof agentId === 'string' && agentId.trim() ? agentId.trim() : null,
            }, 'prompt-inspection');
        } catch (err) {
            logToFile('warn', 'agent-inspect-prompt failed', { error: err?.message ?? String(err), agentId });
            return null;
        }
    });
    ipcMain.handle('agent-query-audits', async (_event, actionTypes, since, limit) => {
        try {
            return await sendAgentQuery({
                type: 'query-audits',
                action_types: Array.isArray(actionTypes) ? actionTypes : null,
                since: Number.isFinite(since) ? Math.trunc(since) : null,
                limit: Number.isFinite(limit) ? Math.max(1, Math.trunc(limit)) : null,
            }, 'audit-list');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-get-provenance-report', async (_event, limit) => {
        try {
            return await sendAgentQuery({
                type: 'get-provenance-report',
                limit: Number.isFinite(limit) ? Math.max(1, Math.trunc(limit)) : null,
            }, 'provenance-report');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-get-memory-provenance-report', async (_event, target, limit) => {
        try {
            return await sendAgentQuery({
                type: 'get-memory-provenance-report',
                target: typeof target === 'string' && target.trim() ? target.trim() : null,
                limit: Number.isFinite(limit) ? Math.max(1, Math.trunc(limit)) : null,
            }, 'memory-provenance-report');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-confirm-memory-provenance-entry', async (_event, entryId) => {
        try {
            return await sendAgentQuery({
                type: 'confirm-memory-provenance-entry',
                entry_id: entryId,
            }, 'memory-provenance-confirmed');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-retract-memory-provenance-entry', async (_event, entryId) => {
        try {
            return await sendAgentQuery({
                type: 'retract-memory-provenance-entry',
                entry_id: entryId,
            }, 'memory-provenance-retracted');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-get-collaboration-sessions', async (_event, parentTaskId) => {
        try {
            return await sendAgentQuery({
                type: 'get-collaboration-sessions',
                parent_task_id: typeof parentTaskId === 'string' && parentTaskId.trim() ? parentTaskId.trim() : null,
            }, 'collaboration-sessions');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-list-generated-tools', async () => {
        try {
            return await sendAgentQuery({ type: 'list-generated-tools' }, 'generated-tools');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-run-generated-tool', async (_event, toolName, argsJson) => {
        try {
            return await sendAgentQuery({
                type: 'run-generated-tool',
                tool_name: toolName,
                args_json: typeof argsJson === 'string' && argsJson.trim() ? argsJson : '{}',
            }, 'generated-tool-result');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-speech-to-text', async (_event, base64Audio, mimeType, options) => {
        try {
            if (typeof saveTempAudioCapture !== 'function') {
                return { error: 'Audio capture persistence is not available' };
            }
            const saved = saveTempAudioCapture({
                base64: typeof base64Audio === 'string' ? base64Audio : '',
                mimeType: typeof mimeType === 'string' && mimeType.trim() ? mimeType.trim() : 'audio/webm',
            });
            if (!saved?.ok || !saved?.path) {
                return { error: saved?.error || 'Failed to persist audio capture' };
            }
            const payload = {
                ...(options && typeof options === 'object' && !Array.isArray(options) ? options : {}),
                path: saved.path,
                mime_type: saved.mimeType || mimeType || 'audio/webm',
            };
            return await sendAgentQuery({
                type: 'speech-to-text',
                args_json: JSON.stringify(payload),
            }, 'speech-to-text-result', 30000);
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-text-to-speech', async (_event, text, voice, options) => {
        try {
            const payload = {
                ...(options && typeof options === 'object' && !Array.isArray(options) ? options : {}),
                input: typeof text === 'string' ? text : '',
            };
            if (typeof voice === 'string' && voice.trim()) {
                payload.voice = voice.trim();
            }
            const result = await sendAgentQuery({
                type: 'text-to-speech',
                args_json: JSON.stringify(payload),
            }, 'text-to-speech-result', 30000);
            if (result && typeof result === 'object' && typeof result.path === 'string' && result.path.trim()) {
                return {
                    ...result,
                    file_url: pathToFileURL(result.path).href,
                };
            }
            return result;
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-activate-generated-tool', async (_event, toolName) => {
        try {
            return await sendAgentQuery({
                type: 'activate-generated-tool',
                tool_name: toolName,
            }, 'generated-tool-result');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-promote-generated-tool', async (_event, toolName) => {
        try {
            return await sendAgentQuery({
                type: 'promote-generated-tool',
                tool_name: toolName,
            }, 'generated-tool-result');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-retire-generated-tool', async (_event, toolName) => {
        try {
            return await sendAgentQuery({
                type: 'retire-generated-tool',
                tool_name: toolName,
            }, 'generated-tool-result');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-vote-on-collaboration-disagreement', async (_event, parentTaskId, disagreementId, taskId, position, confidence) => {
        try {
            return await sendAgentQuery({
                type: 'vote-on-collaboration-disagreement',
                parent_task_id: parentTaskId,
                disagreement_id: disagreementId,
                task_id: taskId,
                position,
                confidence: Number.isFinite(confidence) ? confidence : null,
            }, 'collaboration-vote-result');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-get-operator-model', async () => {
        try {
            return await sendAgentQuery({ type: 'get-operator-model' }, 'operator-model');
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-reset-operator-model', async () => {
        try {
            return await sendAgentQuery({ type: 'reset-operator-model' }, 'operator-model-reset');
        } catch (err) {
            return { ok: false, error: err?.message || String(err) };
        }
    });
    ipcMain.handle('agent-set-config-item', async (_event, keyPath, value) => { try { sendAgentCommand({ type: 'set-config-item', key_path: keyPath, value_json: JSON.stringify(value) }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('agent-set-provider-model', async (_event, providerId, model) => { try { sendAgentCommand({ type: 'set-provider-model', provider_id: providerId, model }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('agent-set-target-agent-provider-model', async (_event, targetAgentId, providerId, model) => { try { sendAgentCommand({ type: 'set-target-agent-provider-model', target_agent_id: targetAgentId, provider_id: providerId, model }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('agent-set-tier-override', async (_event, tier) => { try { sendAgentCommand({ type: 'set-tier-override', tier: tier || null }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('gateway:get-config', async () => { try { const config = await sendAgentQuery({ type: 'get-config' }, 'config'); return config?.gateway ?? {}; } catch (err) { logToFile('warn', '[gateway] get-config IPC error', { error: err.message }); return {}; } });
    ipcMain.handle('gateway:set-config', async (_event, patch) => {
        try {
            for (const [key, value] of Object.entries(patch || {})) {
                sendAgentCommand({ type: 'set-config-item', key_path: `gateway.${key}`, value_json: JSON.stringify(value) });
            }
            logToFile('info', '[gateway] Config updated via IPC', { keys: Object.keys(patch || {}) });
            return { ok: true };
        } catch (err) {
            logToFile('warn', '[gateway] set-config IPC error', { error: err.message });
            return { ok: false, error: err.message };
        }
    });
    ipcMain.handle('agent-get-provider-auth-states', async () => sendAgentQuery({ type: 'get-provider-auth-states' }, 'provider-auth-states'));
    ipcMain.handle('agent-login-provider', async (_event, providerId, apiKey, baseUrl) => { try { return await sendAgentQuery({ type: 'login-provider', provider_id: providerId, api_key: apiKey, base_url: baseUrl || '' }, 'provider-auth-states'); } catch (err) { return { error: err.message }; } });
    ipcMain.handle('agent-logout-provider', async (_event, providerId) => { try { return await sendAgentQuery({ type: 'logout-provider', provider_id: providerId }, 'provider-auth-states'); } catch (err) { return { error: err.message }; } });
    ipcMain.handle('agent-validate-provider', async (_event, providerId, baseUrl, apiKey, authSource) => { try { return await sendAgentQuery({ type: 'validate-provider', provider_id: providerId, base_url: baseUrl, api_key: apiKey, auth_source: authSource }, 'provider-validation'); } catch (err) { return { valid: false, error: err.message }; } });
    ipcMain.handle('agent-fetch-models', async (_event, providerId, baseUrl, apiKey) => { try { return await sendAgentQuery({ type: 'fetch-models', provider_id: providerId, base_url: baseUrl, api_key: apiKey }, 'provider-models'); } catch (err) { return { error: err.message }; } });
    ipcMain.handle('agent-set-sub-agent', async (_event, subAgentJson) => { try { sendAgentCommand({ type: 'set-sub-agent', sub_agent_json: subAgentJson }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('agent-remove-sub-agent', async (_event, subAgentId) => { try { sendAgentCommand({ type: 'remove-sub-agent', sub_agent_id: subAgentId }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('agent-list-sub-agents', async () => { try { return await sendAgentQuery({ type: 'list-sub-agents' }, 'sub-agent-list'); } catch { return []; } });
    ipcMain.handle('openai-codex-auth-status', async (_event, authOptions) => openAICodexAuthHandlers.status(_event, authOptions));
    ipcMain.handle('openai-codex-auth-login', async () => openAICodexAuthHandlers.login());
    ipcMain.handle('openai-codex-auth-logout', async () => openAICodexAuthHandlers.logout());
    ipcMain.handle('agent-heartbeat-get-items', async () => { try { return await sendAgentQuery({ type: 'heartbeat-get-items' }, 'heartbeat-items'); } catch { return []; } });
    ipcMain.handle('agent-heartbeat-set-items', async (_event, items) => { try { sendAgentCommand({ type: 'heartbeat-set-items', items_json: JSON.stringify(items) }); return { ok: true }; } catch (err) { return { ok: false, error: err.message }; } });
    ipcMain.handle('agent-resolve-task-approval', async (_event, approvalId, decision) => {
        try {
            logToFile('info', 'resolving task approval', { approvalId, decision });
            sendAgentCommand({ type: 'resolve-task-approval', approval_id: approvalId, decision });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });
}

module.exports = { registerAgentIpcHandlers };
