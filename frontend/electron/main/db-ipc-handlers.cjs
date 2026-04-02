function registerDbIpcHandlers(ipcMain, runtime) {
    const { sendDbAckCommand, sendDbQuery } = runtime;

    ipcMain.handle('db-append-command-log', async (_event, entry) => {
        try {
            await sendDbAckCommand({ type: 'append-command-log', entry_json: JSON.stringify(entry ?? {}) });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-complete-command-log', async (_event, id, exitCode, durationMs) => {
        try {
            await sendDbAckCommand({
                type: 'complete-command-log',
                id,
                exit_code: Number.isFinite(exitCode) ? Math.trunc(exitCode) : null,
                duration_ms: Number.isFinite(durationMs) ? Math.trunc(durationMs) : null,
            });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-query-command-log', async (_event, opts = {}) => {
        try {
            return await sendDbQuery({
                type: 'query-command-log',
                workspace_id: typeof opts.workspaceId === 'string' ? opts.workspaceId : null,
                pane_id: typeof opts.paneId === 'string' ? opts.paneId : null,
                limit: Number.isFinite(opts.limit) ? Math.max(1, Math.trunc(opts.limit)) : null,
            }, 'command-log-entries');
        } catch {
            return [];
        }
    });
    ipcMain.handle('db-clear-command-log', async () => {
        try {
            await sendDbAckCommand({ type: 'clear-command-log' });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-create-thread', async (_event, thread) => {
        try {
            await sendDbAckCommand({ type: 'create-agent-thread', thread_json: JSON.stringify(thread ?? {}) });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-delete-thread', async (_event, threadId) => {
        try {
            await sendDbAckCommand({ type: 'delete-agent-thread', thread_id: threadId });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-list-threads', async () => {
        try {
            return await sendDbQuery({ type: 'list-agent-threads' }, 'agent-thread-list');
        } catch {
            return [];
        }
    });
    ipcMain.handle('db-get-thread', async (_event, threadId) => {
        try {
            return await sendDbQuery({ type: 'get-agent-thread', thread_id: threadId }, 'agent-thread-detail');
        } catch {
            return { thread: null, messages: [] };
        }
    });
    ipcMain.handle('db-add-message', async (_event, message) => {
        try {
            await sendDbAckCommand({ type: 'add-agent-message', message_json: JSON.stringify(message ?? {}) });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-delete-message', async (_event, threadId, messageId) => {
        try {
            await sendDbAckCommand({ type: 'delete-agent-messages', thread_id: threadId, message_ids: [messageId] });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-list-messages', async (_event, threadId, limit) => {
        try {
            const result = await sendDbQuery({
                type: 'list-agent-messages',
                thread_id: threadId,
                limit: Number.isFinite(limit) ? Math.max(1, Math.trunc(limit)) : null,
            }, 'agent-thread-detail');
            return Array.isArray(result?.messages) ? result.messages : [];
        } catch {
            return [];
        }
    });
    ipcMain.handle('db-upsert-transcript-index', async (_event, entry) => {
        try {
            await sendDbAckCommand({ type: 'upsert-transcript-index', entry_json: JSON.stringify(entry ?? {}) });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-list-transcript-index', async (_event, workspaceId) => {
        try {
            return await sendDbQuery({ type: 'list-transcript-index', workspace_id: typeof workspaceId === 'string' ? workspaceId : null }, 'transcript-index-entries');
        } catch {
            return [];
        }
    });
    ipcMain.handle('db-upsert-snapshot-index', async (_event, entry) => {
        try {
            await sendDbAckCommand({ type: 'upsert-snapshot-index', entry_json: JSON.stringify(entry ?? {}) });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-list-snapshot-index', async (_event, workspaceId) => {
        try {
            return await sendDbQuery({ type: 'list-snapshot-index', workspace_id: typeof workspaceId === 'string' ? workspaceId : null }, 'snapshot-index-entries');
        } catch {
            return [];
        }
    });
    ipcMain.handle('db-upsert-agent-event', async (_event, eventRow) => {
        try {
            await sendDbAckCommand({ type: 'upsert-agent-event', event_json: JSON.stringify(eventRow ?? {}) });
            return true;
        } catch {
            return false;
        }
    });
    ipcMain.handle('db-list-agent-events', async (_event, opts = {}) => {
        try {
            return await sendDbQuery({
                type: 'list-agent-events',
                category: typeof opts.category === 'string' ? opts.category : null,
                pane_id: typeof opts.paneId === 'string' ? opts.paneId : null,
                limit: Number.isFinite(opts.limit) ? Math.max(1, Math.trunc(opts.limit)) : null,
            }, 'agent-event-rows');
        } catch {
            return [];
        }
    });
}

module.exports = { registerDbIpcHandlers };
