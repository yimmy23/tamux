function createWhatsAppRuntime(options) {
    const {
        electronDir,
        fs,
        getChildProcessEnv = () => processRef.env,
        getMainWindow,
        logToFile,
        path,
        pathToFileURL,
        processRef,
        spawn,
    } = options;

    let whatsappProcess = null;
    let whatsappRpcId = 0;
    const whatsappPendingCalls = new Map();
    let whatsappDaemonSubscribed = false;
    let whatsappDaemonSubscriptionDesired = false;
    let desktopWhatsAppLinkingHelpersPromise = null;

    function getDesktopWhatsAppLinkingHelpers() {
        if (!desktopWhatsAppLinkingHelpersPromise) {
            desktopWhatsAppLinkingHelpersPromise = import(pathToFileURL(path.join(electronDir, 'whatsappLinking.js')).href);
        }
        return desktopWhatsAppLinkingHelpersPromise;
    }

    async function handleWhatsAppMessage(msg) {
        if (msg.id !== undefined && whatsappPendingCalls.has(msg.id)) {
            const { resolve, reject } = whatsappPendingCalls.get(msg.id);
            whatsappPendingCalls.delete(msg.id);
            if (msg.error) reject(new Error(msg.error));
            else resolve(msg.result);
            return;
        }

        const mainWindow = getMainWindow();
        if (!msg.event || !mainWindow) return;

        switch (msg.event) {
            case 'qr': {
                logToFile('info', 'WhatsApp bridge qr event', {
                    asciiLen: typeof msg.data?.ascii_qr === 'string' ? msg.data.ascii_qr.length : 0,
                    hasDataUrl: typeof msg.data?.data_url === 'string',
                });
                const { getRendererWhatsAppQrDataUrl } = await getDesktopWhatsAppLinkingHelpers();
                mainWindow.webContents.send('whatsapp-qr', getRendererWhatsAppQrDataUrl(msg.data));
                break;
            }
            case 'connected':
                logToFile('info', 'WhatsApp bridge connected event', { phone: msg.data?.phone || null });
                mainWindow.webContents.send('whatsapp-connected', msg.data);
                break;
            case 'disconnected':
                logToFile('info', 'WhatsApp bridge disconnected event', { reason: msg.data?.reason || null });
                mainWindow.webContents.send('whatsapp-disconnected', { reason: msg.data?.reason || null });
                break;
            case 'error':
                logToFile('warn', 'WhatsApp bridge error event', { message: msg.data || null });
                mainWindow.webContents.send('whatsapp-error', msg.data);
                break;
            case 'message':
                mainWindow.webContents.send('whatsapp-message', msg.data);
                break;
            case 'reconnecting':
                logToFile('info', 'WhatsApp bridge reconnecting...', {
                    statusCode: msg.data?.status_code ?? null,
                    reason: msg.data?.reason ?? null,
                    connectAttempt: msg.data?.connect_attempt ?? null,
                    relinkRetryAttempt: msg.data?.relink_retry_attempt ?? null,
                });
                break;
            case 'ready':
                logToFile('info', 'WhatsApp bridge sidecar ready');
                break;
            case 'trace':
                logToFile('info', 'WhatsApp bridge trace', msg.data ?? {});
                break;
        }
    }

    function startWhatsAppBridge() {
        if (whatsappProcess) return;
        const bridgePath = path.join(electronDir, 'whatsapp-bridge.cjs');
        if (!fs.existsSync(bridgePath)) {
            logToFile('warn', 'whatsapp-bridge.cjs not found');
            throw new Error('WhatsApp bridge script not found');
        }

        logToFile('info', 'starting WhatsApp bridge sidecar');
        whatsappProcess = spawn(processRef.execPath, [bridgePath], {
            stdio: ['pipe', 'pipe', 'pipe'],
            env: { ...getChildProcessEnv(), ELECTRON_RUN_AS_NODE: '1' },
        });

        let buffer = '';
        whatsappProcess.stdout.on('data', (chunk) => {
            buffer += chunk.toString();
            const lines = buffer.split('\n');
            buffer = lines.pop() || '';
            for (const line of lines) {
                if (!line.trim()) continue;
                try {
                    void handleWhatsAppMessage(JSON.parse(line));
                } catch (err) {
                    logToFile('warn', `WhatsApp bridge invalid JSON: ${err.message}`);
                }
            }
        });

        whatsappProcess.stderr.on('data', (chunk) => {
            const text = chunk.toString().trim();
            logToFile('warn', `WhatsApp bridge stderr: ${text}`);
            const mainWindow = getMainWindow();
            if (mainWindow && text) {
                mainWindow.webContents.send('whatsapp-error', `Bridge error: ${text}`);
            }
        });

        whatsappProcess.on('close', (code) => {
            logToFile('info', `WhatsApp bridge exited with code ${code}`);
            whatsappProcess = null;
            for (const [, { reject }] of whatsappPendingCalls) reject(new Error('WhatsApp bridge process exited'));
            whatsappPendingCalls.clear();
            const mainWindow = getMainWindow();
            if (mainWindow && code !== 0) {
                mainWindow.webContents.send('whatsapp-error', `WhatsApp bridge exited unexpectedly (code ${code}). Check that dependencies are installed.`);
                mainWindow.webContents.send('whatsapp-disconnected', null);
            }
        });

        whatsappProcess.on('error', (err) => {
            logToFile('error', `WhatsApp bridge spawn error: ${err.message}`);
            whatsappProcess = null;
        });
    }

    function whatsappRpc(method, params) {
        return new Promise((resolve, reject) => {
            if (!whatsappProcess) return reject(new Error('WhatsApp bridge not running'));
            const id = ++whatsappRpcId;
            whatsappPendingCalls.set(id, { resolve, reject });
            whatsappProcess.stdin.write(`${JSON.stringify({ id, method, params })}\n`);
            setTimeout(() => {
                if (!whatsappPendingCalls.has(id)) return;
                whatsappPendingCalls.delete(id);
                reject(new Error('WhatsApp RPC timeout'));
            }, 60000);
        });
    }

    function stopWhatsAppBridge() {
        if (whatsappProcess) {
            whatsappProcess.kill('SIGTERM');
            whatsappProcess = null;
        }
    }

    async function queryGatewayConfig(sendAgentQuery) {
        return await sendAgentQuery({ type: 'get-gateway-config' }, 'gateway-config');
    }

    function registerWhatsAppIpcHandlers(ipcMain, runtime, validateConnectConfig) {
        const { sendAgentCommand, sendAgentQuery } = runtime;
        ipcMain.handle('whatsapp-connect', async () => {
            try {
                const gatewayConfig = await queryGatewayConfig(sendAgentQuery);
                await validateConnectConfig({ gateway: gatewayConfig ?? {} });
                if (gatewayConfig?.whatsapp_link_fallback_electron === true) {
                    startWhatsAppBridge();
                    await whatsappRpc('connect');
                    return { ok: true };
                }
                if (!whatsappDaemonSubscribed) {
                    sendAgentCommand({ type: 'whats-app-link-subscribe' });
                    whatsappDaemonSubscribed = true;
                    whatsappDaemonSubscriptionDesired = true;
                }
                sendAgentCommand({ type: 'whats-app-link-start' });
                return { ok: true };
            } catch (err) {
                return { ok: false, error: err.message };
            }
        });
        ipcMain.handle('whatsapp-disconnect', async () => {
            try {
                const gatewayConfig = await queryGatewayConfig(sendAgentQuery);
                if (gatewayConfig?.whatsapp_link_fallback_electron === true) {
                    if (whatsappProcess) await whatsappRpc('disconnect').catch(() => {});
                    stopWhatsAppBridge();
                    return { ok: true };
                }
                sendAgentCommand({ type: 'whats-app-link-stop' });
                if (whatsappDaemonSubscribed) {
                    sendAgentCommand({ type: 'whats-app-link-unsubscribe' });
                    whatsappDaemonSubscribed = false;
                }
                whatsappDaemonSubscriptionDesired = false;
                return { ok: true };
            } catch (err) {
                return { ok: false, error: err.message };
            }
        });
        ipcMain.handle('whatsapp-status', async () => {
            try {
                const gatewayConfig = await queryGatewayConfig(sendAgentQuery);
                if (gatewayConfig?.whatsapp_link_fallback_electron === true) {
                    if (!whatsappProcess) return { status: 'disconnected', phone: null };
                    return await whatsappRpc('status');
                }
                const status = await sendAgentQuery({ type: 'whats-app-link-status' }, 'whatsapp-link-status');
                return {
                    status: status?.status ?? 'disconnected',
                    phone: status?.phone ?? null,
                    phoneNumber: status?.phone ?? null,
                    lastError: status?.last_error ?? null,
                };
            } catch (error) {
                return { status: 'error', phone: null, phoneNumber: null, lastError: error.message || String(error) };
            }
        });
        ipcMain.handle('whatsapp-send', async (_event, jid, text) => {
            const target = typeof jid === 'string' ? jid.trim() : '';
            const message = typeof text === 'string' ? text : '';
            if (!target || !message) {
                return { ok: false, error: 'whatsapp-send requires jid and text' };
            }
            return { ok: false, error: 'whatsapp-send is disabled in Electron; daemon gateway messaging is authoritative' };
        });
    }

    return {
        getDesktopWhatsAppLinkingHelpers,
        isDaemonSubscribed: () => whatsappDaemonSubscribed,
        isSubscriptionRestoreNeeded: () => whatsappDaemonSubscriptionDesired && !whatsappDaemonSubscribed,
        registerWhatsAppIpcHandlers,
        setDaemonSubscribed: (value) => { whatsappDaemonSubscribed = Boolean(value); },
        stopWhatsAppBridge,
        stopWhatsAppRuntime: () => {
            whatsappDaemonSubscribed = false;
            whatsappDaemonSubscriptionDesired = false;
            stopWhatsAppBridge();
        },
    };
}

module.exports = { createWhatsAppRuntime };
