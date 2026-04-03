function createAgentDbBridgeRuntime(options) {
    const {
        fs,
        getDaemonPath,
        getMainWindow,
        logToFile,
        pendingHandlerMatchesResponseType,
        resolvePendingAgentQueryEvent,
        sendRenderedWhatsAppQr,
        setWhatsAppSubscribed,
        shouldRestoreWhatsAppSubscription,
        spawn,
    } = options;

    let agentBridge = null;
    let dbBridge = null;
    let consecutiveAgentBridgeExits = 0;
    let agentBridgeRestartCooldownUntil = 0;

    function getCliPath() {
        return getDaemonPath().replace(/tamux-daemon/, 'tamux').replace(/tamux-daemon\.exe/, 'tamux.exe');
    }

    function resolveOldestPending(pending, matcher = null) {
        let oldest = null;
        for (const [reqId, handler] of pending.entries()) {
            if (matcher && !matcher(handler)) continue;
            if (!oldest || handler.ts < oldest.ts) {
                oldest = { reqId, handler, ts: handler.ts };
            }
        }
        return oldest;
    }

    function responseTypesOverlap(leftResponseType, rightResponseType) {
        const left = Array.isArray(leftResponseType) ? leftResponseType : [leftResponseType];
        const right = Array.isArray(rightResponseType) ? rightResponseType : [rightResponseType];
        return left.some((eventType) => right.includes(eventType));
    }

    function hasPendingResponseType(pending, responseType) {
        for (const [, handler] of pending.entries()) {
            if (responseTypesOverlap(handler.responseType, responseType)) {
                return true;
            }
        }
        return false;
    }

    function ensureAgentBridge() {
        if (agentBridge && !agentBridge.process.killed) return agentBridge;
        if (Date.now() < agentBridgeRestartCooldownUntil) return null;

        const cliPath = getCliPath();
        if (!fs.existsSync(cliPath)) {
            logToFile('warn', 'agent bridge: tamux CLI not found', { cliPath });
            return null;
        }

        const bridgeProcess = spawn(cliPath, ['agent-bridge'], {
            cwd: require('path').dirname(cliPath),
            windowsHide: true,
            stdio: ['pipe', 'pipe', 'pipe'],
        });

        agentBridge = { process: bridgeProcess, ready: false, pending: new Map(), stdoutBuffer: '' };

        bridgeProcess.stdout.on('data', (chunk) => {
            agentBridge.stdoutBuffer += chunk.toString('utf8');
            const lines = agentBridge.stdoutBuffer.split(/\r?\n/);
            agentBridge.stdoutBuffer = lines.pop() ?? '';

            for (const line of lines) {
                if (!line.trim()) continue;
                let event;
                try {
                    event = JSON.parse(line);
                } catch {
                    continue;
                }

                if (event.type === 'ready') {
                    agentBridge.ready = true;
                    consecutiveAgentBridgeExits = 0;
                    agentBridgeRestartCooldownUntil = 0;
                    logToFile('info', 'agent bridge ready');
                    if (shouldRestoreWhatsAppSubscription()) {
                        try {
                            sendAgentCommand({ type: 'whats-app-link-subscribe' });
                            setWhatsAppSubscribed(true);
                            logToFile('info', 'restored daemon WhatsApp subscription after bridge ready');
                        } catch (error) {
                            logToFile('warn', 'failed to restore daemon WhatsApp subscription', { error: error?.message || String(error) });
                        }
                    }
                    continue;
                }

                if (event.type === 'concierge_welcome') {
                    logToFile('info', '[concierge] received concierge_welcome from bridge', { hasContent: !!event.content, hasActions: !!event.actions });
                }
                if (resolvePendingAgentQueryEvent(agentBridge, event)) {
                    continue;
                }

                if (event.type === 'error') {
                    const oldest = resolveOldestPending(agentBridge.pending);
                    if (oldest) {
                        const msg = event.message || event.data?.message || (typeof event.data === 'string' ? event.data : null) || 'agent bridge error';
                        oldest.handler.reject(new Error(msg));
                        agentBridge.pending.delete(oldest.reqId);
                        continue;
                    }
                }

                if (event.type === 'plugin-oauth-complete') {
                    const mainWindow = getMainWindow();
                    if (mainWindow && !mainWindow.isDestroyed()) {
                        mainWindow.webContents.send('plugin-oauth-complete', {
                            name: event.name,
                            success: event.success,
                            error: event.error,
                        });
                    }
                    continue;
                }

                if (event.type === 'whatsapp-link-status') {
                    const oldest = resolveOldestPending(agentBridge.pending, (handler) => pendingHandlerMatchesResponseType(handler, event.type));
                    if (oldest) {
                        oldest.handler.resolve(event.data ?? event);
                        agentBridge.pending.delete(oldest.reqId);
                    }
                    const lastError = event.data?.last_error;
                    const mainWindow = getMainWindow();
                    if (mainWindow && !mainWindow.isDestroyed() && typeof lastError === 'string' && lastError.trim()) {
                        mainWindow.webContents.send('whatsapp-error', lastError);
                    }
                    continue;
                }

                if (event.type === 'whatsapp-link-qr') {
                    void sendRenderedWhatsAppQr(event.data?.ascii_qr);
                    continue;
                }

                if (event.type === 'whatsapp-link-linked') {
                    const mainWindow = getMainWindow();
                    if (mainWindow && !mainWindow.isDestroyed()) {
                        mainWindow.webContents.send('whatsapp-connected', { phone: event.data?.phone || null });
                    }
                    continue;
                }

                if (event.type === 'whatsapp-link-error') {
                    const mainWindow = getMainWindow();
                    if (mainWindow && !mainWindow.isDestroyed()) {
                        mainWindow.webContents.send('whatsapp-error', event.data?.message || 'WhatsApp link error');
                    }
                    continue;
                }

                if (event.type === 'whatsapp-link-disconnected') {
                    const reason = event.data?.reason;
                    const mainWindow = getMainWindow();
                    if (mainWindow && !mainWindow.isDestroyed()) {
                        if (typeof reason === 'string' && reason.trim()) {
                            mainWindow.webContents.send('whatsapp-error', reason);
                        }
                        mainWindow.webContents.send('whatsapp-disconnected', {
                            reason: typeof reason === 'string' ? reason : null,
                        });
                    }
                    continue;
                }

                const mainWindow = getMainWindow();
                if (event.type === 'concierge_welcome') {
                    logToFile('info', '[concierge] forwarding concierge_welcome to renderer', { contentLen: event.content?.length, actionsLen: event.actions?.length });
                }
                if (mainWindow && !mainWindow.isDestroyed()) {
                    mainWindow.webContents.send('agent-event', event);
                } else if (event.type === 'concierge_welcome') {
                    logToFile('warn', '[concierge] mainWindow not available to forward event');
                }
            }
        });

        bridgeProcess.stderr.on('data', (chunk) => {
            logToFile('warn', 'agent bridge stderr', { message: chunk.toString('utf8').trim() });
        });

        bridgeProcess.on('exit', (code) => {
            logToFile('info', 'agent bridge exited', { code });
            for (const [, handler] of (agentBridge?.pending ?? new Map()).entries()) {
                handler.reject(new Error('agent bridge exited'));
            }
            agentBridge = null;
            setWhatsAppSubscribed(false);
            consecutiveAgentBridgeExits += 1;
            const cappedBackoffMs = Math.min(5000, 250 * (2 ** Math.min(consecutiveAgentBridgeExits, 5)));
            agentBridgeRestartCooldownUntil = Date.now() + cappedBackoffMs;
            logToFile('warn', 'agent bridge restart cooldown', { consecutiveExits: consecutiveAgentBridgeExits, cooldownMs: cappedBackoffMs });
        });

        return agentBridge;
    }

    function ensureDbBridge() {
        if (dbBridge && !dbBridge.process.killed) return dbBridge;
        const cliPath = getCliPath();
        if (!fs.existsSync(cliPath)) {
            logToFile('warn', 'db bridge: tamux CLI not found', { cliPath });
            return null;
        }

        const bridgeProcess = spawn(cliPath, ['db-bridge'], {
            cwd: require('path').dirname(cliPath),
            windowsHide: true,
            stdio: ['pipe', 'pipe', 'pipe'],
        });

        dbBridge = { process: bridgeProcess, ready: false, pending: new Map(), stdoutBuffer: '' };

        bridgeProcess.stdout.on('data', (chunk) => {
            dbBridge.stdoutBuffer += chunk.toString('utf8');
            const lines = dbBridge.stdoutBuffer.split(/\r?\n/);
            dbBridge.stdoutBuffer = lines.pop() ?? '';
            for (const line of lines) {
                if (!line.trim()) continue;
                let event;
                try {
                    event = JSON.parse(line);
                } catch {
                    continue;
                }
                if (event.type === 'ready') {
                    dbBridge.ready = true;
                    continue;
                }
                if (event.type === 'error') {
                    const oldest = resolveOldestPending(dbBridge.pending);
                    if (oldest) {
                        const msg = event.message || event.data?.message || (typeof event.data === 'string' ? event.data : null) || 'db bridge error';
                        oldest.handler.reject(new Error(msg));
                        dbBridge.pending.delete(oldest.reqId);
                    }
                    logToFile('warn', 'db bridge error', { message: event.message || event.data });
                    continue;
                }
                const oldest = resolveOldestPending(dbBridge.pending, (handler) => handler.responseType === event.type);
                if (oldest) {
                    oldest.handler.resolve(event.type === 'ack' ? true : event.data ?? { thread: event.thread ?? null, messages: event.messages ?? [] });
                    dbBridge.pending.delete(oldest.reqId);
                }
            }
        });

        bridgeProcess.stderr.on('data', (chunk) => {
            logToFile('warn', 'db bridge stderr', { message: chunk.toString('utf8').trim() });
        });

        bridgeProcess.on('exit', () => {
            for (const [, handler] of (dbBridge?.pending ?? new Map()).entries()) {
                handler.reject(new Error('db bridge exited'));
            }
            dbBridge = null;
        });
        return dbBridge;
    }

    function sendAgentCommand(command) {
        const bridge = ensureAgentBridge();
        if (!bridge || bridge.process.killed || !bridge.process.stdin.writable) {
            throw new Error('Agent bridge not available. Is the daemon running?');
        }
        bridge.process.stdin.write(`${JSON.stringify(command)}\n`);
    }

    function sendAgentQuery(command, responseType, timeoutMs = 5000) {
        return new Promise((resolve, reject) => {
            const bridge = ensureAgentBridge();
            if (!bridge) return reject(new Error('Agent bridge not available'));
            if (hasPendingResponseType(bridge.pending, responseType)) {
                return reject(new Error(`Agent query already pending for response type: ${Array.isArray(responseType) ? responseType.join('|') : responseType}`));
            }
            const responseKey = Array.isArray(responseType) ? responseType.join('|') : responseType;
            const reqId = `${responseKey}_${Date.now()}_${Math.random()}`;
            const timer = setTimeout(() => {
                bridge.pending.delete(reqId);
                reject(new Error(`Agent query timeout: ${responseKey}`));
            }, timeoutMs);
            bridge.pending.set(reqId, {
                responseType,
                ts: Date.now(),
                resolve: (data) => { clearTimeout(timer); resolve(data); },
                reject: (err) => { clearTimeout(timer); reject(err); },
            });
            sendAgentCommand(command);
        });
    }

    function sendDbQuery(command, responseType, timeoutMs = 5000) {
        return new Promise((resolve, reject) => {
            const bridge = ensureDbBridge();
            if (!bridge) return reject(new Error('DB bridge not available'));
            const reqId = `${responseType}_${Date.now()}_${Math.random()}`;
            const timer = setTimeout(() => {
                bridge.pending.delete(reqId);
                reject(new Error(`DB query timeout: ${responseType}`));
            }, timeoutMs);
            bridge.pending.set(reqId, {
                responseType,
                ts: Date.now(),
                resolve: (data) => { clearTimeout(timer); resolve(data); },
                reject: (err) => { clearTimeout(timer); reject(err); },
            });
            bridge.process.stdin.write(`${JSON.stringify(command)}\n`);
        });
    }

    return {
        sendAgentCommand,
        sendAgentQuery,
        sendDbAckCommand: (command, timeoutMs = 5000) => sendDbQuery(command, 'ack', timeoutMs),
        sendDbQuery,
    };
}

module.exports = { createAgentDbBridgeRuntime };
