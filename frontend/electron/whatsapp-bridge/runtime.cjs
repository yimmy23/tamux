const QRCode = require('qrcode');
const {
    AUTH_DIR,
    logger,
    state,
    installStdoutGuards,
    clearReconnectTimer,
    scheduleReconnect,
    shouldTreatAsTerminalDisconnect,
    resetAuthState,
    shouldRetryTerminalRelink,
    rememberOutboundMessageId,
    isRecentOutboundMessageId,
    ownPhoneDigits,
    ownExactJidCandidates,
    pushUniqueTarget,
    resolveSendJidCandidates,
    getBaileysApi,
    sendEvent,
    emitTrace,
    summarizeReason,
    extractMessageText,
    collectOwnIdentifiers,
    isSelfChatRemoteJid,
    sendResult,
    sendError,
} = require('./core.cjs');

function createBridgeRuntime() {
    async function recoverClosedSessionState() {
        if (state.closedSessionRecoveryInFlight) return;
        if (!state.sock || !state.isConnected) return;
        if (typeof state.sock.assertSessions !== 'function') return;
        state.closedSessionRecoveryInFlight = true;
        try {
            const ownPhone = ownPhoneDigits();
            const targets = [];
            for (const jid of ownExactJidCandidates()) {
                pushUniqueTarget(targets, jid);
            }
            if (ownPhone) {
                pushUniqueTarget(targets, `${ownPhone}@s.whatsapp.net`);
                pushUniqueTarget(targets, `${ownPhone}@lid`);
            }
            if (targets.length === 0) {
                emitTrace('closed_session_recovery_skipped', {
                    reason: 'no_targets',
                });
                return;
            }
            emitTrace('closed_session_recovery_attempt', { targets });
            const result = await state.sock.assertSessions(targets, true);
            emitTrace('closed_session_recovery_result', {
                targets,
                result: Boolean(result),
            });
        } catch (error) {
            emitTrace('closed_session_recovery_failed', {
                error: error?.message || String(error),
            });
        } finally {
            state.closedSessionRecoveryInFlight = false;
        }
    }

    installStdoutGuards({ onClosedSessionWarning: recoverClosedSessionState });

    async function connectWhatsApp() {
        if (state.sock) {
            emitTrace('connect_ignored_already_active', {
                is_connected: state.isConnected,
                connect_attempt: state.connectAttempt,
            });
            return;
        }
        clearReconnectTimer();
        state.connectAttempt += 1;
        emitTrace('connect_attempt', {
            connect_attempt: state.connectAttempt,
            relink_retry_attempt: state.reconnectAttempt,
        });

        const {
            makeWASocket,
            Browsers,
            fetchLatestBaileysVersion,
            DisconnectReason,
            useMultiFileAuthState,
            makeCacheableSignalKeyStore,
        } = await getBaileysApi();
        const { state: authState, saveCreds } = await useMultiFileAuthState(AUTH_DIR);
        const { version } = await fetchLatestBaileysVersion();
        emitTrace('baileys_version', {
            version,
            connect_attempt: state.connectAttempt,
        });

        state.sock = makeWASocket({
            version,
            auth: {
                creds: authState.creds,
                keys: makeCacheableSignalKeyStore(authState.keys, logger),
            },
            printQRInTerminal: false,
            logger,
            browser: Browsers.ubuntu('Chrome'),
            generateHighQualityLinkPreview: false,
        });

        state.sock.ev.on('creds.update', saveCreds);

        state.sock.ev.on('connection.update', async (update) => {
            const { connection, lastDisconnect, qr } = update;
            emitTrace('connection_update', {
                connection: connection || null,
                connect_attempt: state.connectAttempt,
                has_qr: Boolean(qr),
            });

            if (qr) {
                try {
                    const asciiQr = await QRCode.toString(qr, {
                        type: 'utf8',
                        margin: 2,
                    });
                    const dataUrl = await QRCode.toDataURL(qr, {
                        width: 256,
                        margin: 2,
                        color: { dark: '#000000', light: '#ffffff' },
                    });
                    sendEvent('qr', {
                        ascii_qr: asciiQr,
                        data_url: dataUrl,
                        connect_attempt: state.connectAttempt,
                    });
                    emitTrace('qr_generated', {
                        connect_attempt: state.connectAttempt,
                        ascii_len: asciiQr.length,
                        has_data_url: true,
                    });
                } catch (err) {
                    sendEvent('error', `QR generation failed: ${err.message}`);
                    emitTrace('qr_generation_failed', {
                        connect_attempt: state.connectAttempt,
                        error: err.message || String(err),
                    });
                }
            }

            if (connection === 'close') {
                state.isConnected = false;
                state.sock = null;
                const statusCode = (lastDisconnect?.error)?.output?.statusCode;
                const numericStatusCode = Number.isFinite(statusCode) ? statusCode : null;
                const reconnectReason = summarizeReason(
                    lastDisconnect?.error?.message
                    || lastDisconnect?.error?.toString?.()
                    || null
                );
                const reconnectData = lastDisconnect?.error?.data ?? null;
                const terminalDisconnect = shouldTreatAsTerminalDisconnect(
                    numericStatusCode,
                    reconnectReason,
                    DisconnectReason
                );
                emitTrace('connection_closed', {
                    status_code: numericStatusCode,
                    reason: reconnectReason,
                    reconnect_data: reconnectData,
                    terminal_disconnect: terminalDisconnect,
                    connect_attempt: state.connectAttempt,
                });

                if (!terminalDisconnect) {
                    sendEvent('reconnecting', {
                        reason: reconnectReason,
                        status_code: numericStatusCode,
                        relink_retry_attempt: state.reconnectAttempt + 1,
                        connect_attempt: state.connectAttempt,
                    });
                    scheduleReconnect(connectWhatsApp, sendEvent);
                    return;
                }

                const resetError = resetAuthState();
                if (resetError) {
                    clearReconnectTimer();
                    sendEvent(
                        'error',
                        `Failed to reset WhatsApp auth state: ${resetError.message || String(resetError)}`
                    );
                    sendEvent('disconnected', {
                        reason: reconnectReason || 'auth_reset_failed',
                        status_code: numericStatusCode,
                        connect_attempt: state.connectAttempt,
                    });
                    return;
                }

                if (shouldRetryTerminalRelink()) {
                    sendEvent('reconnecting', {
                        reason: reconnectReason || 'terminal_relink_retry',
                        status_code: numericStatusCode,
                        relink_retry_attempt: state.reconnectAttempt,
                        connect_attempt: state.connectAttempt,
                    });
                    emitTrace('terminal_relink_retry', {
                        status_code: numericStatusCode,
                        reason: reconnectReason,
                        relink_retry_attempt: state.reconnectAttempt,
                        connect_attempt: state.connectAttempt,
                    });
                    scheduleReconnect(connectWhatsApp, sendEvent);
                    return;
                }

                clearReconnectTimer();
                const reasonParts = [];
                if (numericStatusCode !== null) {
                    reasonParts.push(`status_code=${numericStatusCode}`);
                }
                if (reconnectReason) {
                    reasonParts.push(reconnectReason);
                }
                sendEvent(
                    'error',
                    `WhatsApp session requires relink${reasonParts.length ? ` (${reasonParts.join('; ')})` : ''}`
                );
                sendEvent('disconnected', {
                    reason: reconnectReason || null,
                    status_code: numericStatusCode,
                    connect_attempt: state.connectAttempt,
                });
                return;
            }

            if (connection === 'open') {
                clearReconnectTimer();
                state.reconnectAttempt = 0;
                state.isConnected = true;
                emitTrace('connected', {
                    connect_attempt: state.connectAttempt,
                    user_id: state.sock.user?.id || null,
                    user_lid: state.sock.user?.lid || null,
                });
                const phoneNumber = state.sock.user?.id?.split(':')[0] || 'Unknown';
                sendEvent('connected', { phone: `+${phoneNumber}` });
            }
        });

        state.sock.ev.on('messages.upsert', (upsert) => {
            emitTrace('messages_upsert_received', {
                upsert_type: upsert?.type || null,
                count: Array.isArray(upsert?.messages) ? upsert.messages.length : 0,
            });
            if (upsert.type !== 'notify' && upsert.type !== 'append') {
                emitTrace('messages_upsert_skipped_type', {
                    upsert_type: upsert?.type || null,
                    count: Array.isArray(upsert?.messages) ? upsert.messages.length : 0,
                });
                return;
            }

            for (const msg of upsert.messages) {
                const from = msg?.key?.remoteJid || 'unknown';
                const messageId = msg?.key?.id || null;
                const participantJid = msg?.key?.participant || null;
                if (msg?.key?.fromMe) {
                    const selfChat = isSelfChatRemoteJid(from, participantJid);
                    const knownOutboundEcho = isRecentOutboundMessageId(messageId);
                    if (knownOutboundEcho || !selfChat) {
                        emitTrace('message_skipped_from_me', {
                            from,
                            participant: participantJid,
                            message_id: messageId,
                            self_chat: selfChat,
                            known_outbound_echo: knownOutboundEcho,
                            own_ids: Array.from(collectOwnIdentifiers()),
                        });
                        continue;
                    }
                    emitTrace('message_from_me_self_chat_allowed', {
                        from,
                        participant: participantJid,
                        message_id: messageId,
                    });
                }
                if (!from || from === 'status@broadcast') {
                    continue;
                }
                const text = extractMessageText(msg?.message);
                if (!text || !text.trim()) {
                    emitTrace('message_skipped_no_text', {
                        from,
                        message_id: messageId,
                    });
                    continue;
                }
                const pushName = msg.pushName || '';
                sendEvent('message', {
                    from,
                    pushName,
                    text: text.trim(),
                    timestamp: msg.messageTimestamp,
                    messageId,
                });
                emitTrace('message_forwarded', {
                    from,
                    message_id: messageId,
                    text_len: text.trim().length,
                });
            }
        });

        state.sock.ev.on('messages.update', (updates) => {
            const entries = Array.isArray(updates) ? updates : [];
            for (const entry of entries) {
                const messageId = entry?.key?.id || null;
                const remoteJid = entry?.key?.remoteJid || null;
                const participant = entry?.key?.participant || null;
                const tracked = isRecentOutboundMessageId(messageId);
                if (!tracked) {
                    if (!isSelfChatRemoteJid(remoteJid, participant)) continue;
                    emitTrace('outbound_message_update_untracked', {
                        message_id: messageId,
                        remote_jid: remoteJid,
                        participant,
                        status: entry?.update?.status ?? null,
                        update_keys: Object.keys(entry?.update || {}),
                    });
                    continue;
                }
                emitTrace('outbound_message_update', {
                    message_id: messageId,
                    remote_jid: remoteJid,
                    participant,
                    status: entry?.update?.status ?? null,
                    update_keys: Object.keys(entry?.update || {}),
                });
            }
        });

        state.sock.ev.on('message-receipt.update', (updates) => {
            const entries = Array.isArray(updates) ? updates : [updates];
            for (const entry of entries) {
                const messageId = entry?.key?.id || null;
                const remoteJid = entry?.key?.remoteJid || null;
                const participant = entry?.key?.participant || null;
                const tracked = isRecentOutboundMessageId(messageId);
                if (!tracked) {
                    if (!isSelfChatRemoteJid(remoteJid, participant)) continue;
                    emitTrace('outbound_message_receipt_update_untracked', {
                        message_id: messageId,
                        remote_jid: remoteJid,
                        participant,
                        receipt_type: entry?.receipt?.type || null,
                        receipt_user: entry?.receipt?.userJid || null,
                    });
                    continue;
                }
                emitTrace('outbound_message_receipt_update', {
                    message_id: messageId,
                    remote_jid: remoteJid,
                    participant,
                    receipt_type: entry?.receipt?.type || null,
                    receipt_user: entry?.receipt?.userJid || null,
                });
            }
        });
    }

    async function disconnectWhatsApp() {
        clearReconnectTimer();
        state.reconnectAttempt = 0;
        state.connectAttempt = 0;
        if (state.sock) {
            await state.sock.logout().catch(() => {});
            state.sock = null;
            state.isConnected = false;
        }
        emitTrace('manual_disconnect', {});
    }

    function getStatus() {
        if (!state.sock) return { status: 'disconnected', phone: null };
        if (state.isConnected) {
            const phoneNumber = state.sock.user?.id?.split(':')[0] || null;
            return {
                status: 'connected',
                phone: phoneNumber ? `+${phoneNumber}` : null,
            };
        }
        return { status: 'connecting', phone: null };
    }

    async function sendWhatsAppMessage(jid, text) {
        if (!state.sock || !state.isConnected) {
            throw new Error('WhatsApp not connected');
        }
        const targets = resolveSendJidCandidates(jid);
        if (targets.length === 0) {
            throw new Error('WhatsApp send target is empty');
        }
        emitTrace('outbound_send_targets', {
            requested_jid: jid,
            targets,
        });

        if (typeof state.sock.assertSessions === 'function') {
            try {
                const asserted = await state.sock.assertSessions(targets, true);
                emitTrace('outbound_assert_sessions_result', {
                    requested_jid: jid,
                    targets,
                    asserted: Boolean(asserted),
                });
            } catch (error) {
                emitTrace('outbound_assert_sessions_failed', {
                    requested_jid: jid,
                    targets,
                    error: error?.message || String(error),
                });
            }
        }

        let lastError = null;
        for (let i = 0; i < targets.length; i += 1) {
            const target = targets[i];
            emitTrace('outbound_send_attempt', {
                requested_jid: jid,
                target_jid: target,
                attempt: i + 1,
                total_attempts: targets.length,
            });
            try {
                const response = await state.sock.sendMessage(target, { text });
                const outboundMessageId = response?.key?.id;
                rememberOutboundMessageId(outboundMessageId, target);
                emitTrace('outbound_send_success', {
                    requested_jid: jid,
                    target_jid: target,
                    message_id: outboundMessageId || null,
                    response_remote_jid: response?.key?.remoteJid || null,
                    response_from_me: response?.key?.fromMe === true,
                });
                emitTrace('outbound_message_recorded', {
                    message_id: outboundMessageId || null,
                    requested_jid: jid,
                    target_jid: target,
                });
                return;
            } catch (error) {
                lastError = error;
                emitTrace('outbound_send_failed', {
                    requested_jid: jid,
                    target_jid: target,
                    attempt: i + 1,
                    total_attempts: targets.length,
                    error: error?.message || String(error),
                });
            }
        }

        throw lastError || new Error('WhatsApp send failed');
    }

    async function handleCommand(msg) {
        const { id, method, params } = msg;

        try {
            switch (method) {
                case 'connect':
                    sendResult(id, 'ok');
                    if (state.sock) {
                        emitTrace('connect_command_ignored', {
                            is_connected: state.isConnected,
                            connect_attempt: state.connectAttempt,
                        });
                        break;
                    }
                    connectWhatsApp().catch((err) => {
                        sendEvent('error', `Connection failed: ${err.message || String(err)}`);
                    });
                    break;
                case 'disconnect':
                    await disconnectWhatsApp();
                    sendResult(id, 'ok');
                    break;
                case 'status':
                    sendResult(id, getStatus());
                    break;
                case 'send':
                    await sendWhatsAppMessage(params.jid, params.text);
                    sendResult(id, 'ok');
                    break;
                case 'ping':
                    sendResult(id, 'pong');
                    break;
                default:
                    sendError(id, `Unknown method: ${method}`);
            }
        } catch (err) {
            sendError(id, err.message || String(err));
        }
    }

    async function shutdown() {
        clearReconnectTimer();
        if (state.sock) {
            await state.sock.end(undefined).catch(() => {});
        }
    }

    return {
        handleCommand,
        shutdown,
    };
}

module.exports = {
    createBridgeRuntime,
};
