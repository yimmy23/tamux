/**
 * WhatsApp Web bridge sidecar for tamux-gateway.
 *
 * Uses @whiskeysockets/baileys to connect to WhatsApp via the multi-device
 * protocol. Communicates with the Electron main process via JSON-RPC over
 * stdin/stdout.
 *
 * Protocol:
 *   Main -> Bridge:  { "id": 1, "method": "connect" }
 *   Bridge -> Main:  { "id": 1, "result": "ok" }
 *   Bridge -> Main:  {
 *     "event": "qr",
 *     "data": { "ascii_qr": "...", "data_url": "data:image/png;base64,..." }
 *   }
 *   Bridge -> Main:  { "event": "connected", "data": { "phone": "+1234..." } }
 *   Bridge -> Main:  { "event": "message", "data": { "from": "...", ... } }
 *   Bridge -> Main:  { "event": "disconnected" }
 *   Bridge -> Main:  { "event": "error", "data": "..." }
 */

const path = require('path');
const fs = require('fs');
const os = require('os');
const util = require('util');
const QRCode = require('qrcode');
const pino = require('pino');

// Auth state directory
const AUTH_DIR = path.join(
    process.platform === 'win32' && process.env.LOCALAPPDATA
        ? path.join(process.env.LOCALAPPDATA, 'tamux')
        : path.join(os.homedir(), '.tamux'),
    'whatsapp-auth'
);
fs.mkdirSync(AUTH_DIR, { recursive: true });

const logger = pino({ level: 'silent' });
const protocolStdoutWrite = process.stdout.write.bind(process.stdout);

function relayConsoleToStderr(method, args) {
    let rendered = '';
    try {
        rendered = util.format(...args);
        process.stderr.write(`[wa-sidecar:${method}] ${rendered}\n`);
    } catch {
        process.stderr.write(`[wa-sidecar:${method}] <unprintable>\n`);
        rendered = '<unprintable>';
    }

    if (
        method === 'warn'
        && /decrypted message with closed session/i.test(rendered)
    ) {
        recoverClosedSessionState().catch(() => {});
    }
}

for (const method of ['log', 'info', 'warn', 'error', 'debug']) {
    console[method] = (...args) => relayConsoleToStderr(method, args);
}

// Guard protocol stdout: any third-party direct stdout writes are treated as
// diagnostics and redirected to stderr so daemon JSON-RPC parsing remains clean.
process.stdout.write = (chunk, encoding, callback) => {
    try {
        const text = Buffer.isBuffer(chunk) ? chunk.toString('utf8') : String(chunk ?? '');
        if (text.trim()) {
            process.stderr.write(`[wa-sidecar:stdout-noise] ${text}`);
        }
    } catch {
        process.stderr.write('[wa-sidecar:stdout-noise] <unprintable>\n');
    }

    if (typeof encoding === 'function') {
        encoding();
    } else if (typeof callback === 'function') {
        callback();
    }
    return true;
};

let sock = null;
let isConnected = false;
let baileysApi = null;
let reconnectTimer = null;
let reconnectAttempt = 0;
let connectAttempt = 0;
const TERMINAL_RELINK_MAX_RETRIES = 1;
const OUTBOUND_ECHO_TTL_MS = 5 * 60 * 1000;
const outboundMessageIds = new Map();
let closedSessionRecoveryInFlight = false;

function clearReconnectTimer() {
    if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
    }
}

function scheduleReconnect() {
    if (reconnectTimer) return;
    reconnectTimer = setTimeout(() => {
        reconnectTimer = null;
        connectWhatsApp().catch((err) => {
            sendEvent('error', `Reconnect failed: ${err.message || String(err)}`);
        });
    }, 3000);
}

async function recoverClosedSessionState() {
    if (closedSessionRecoveryInFlight) return;
    if (!sock || !isConnected) return;
    if (typeof sock.assertSessions !== 'function') return;
    closedSessionRecoveryInFlight = true;
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
        emitTrace('closed_session_recovery_attempt', {
            targets,
        });
        const result = await sock.assertSessions(targets, true);
        emitTrace('closed_session_recovery_result', {
            targets,
            result: Boolean(result),
        });
    } catch (error) {
        emitTrace('closed_session_recovery_failed', {
            error: error?.message || String(error),
        });
    } finally {
        closedSessionRecoveryInFlight = false;
    }
}

function shouldTreatAsTerminalDisconnect(statusCode, reason, DisconnectReason) {
    if (statusCode === DisconnectReason.loggedOut) return true;
    if ([401, 403, 405].includes(statusCode)) return true;
    if (typeof reason === 'string' && /connection failure/i.test(reason)) return true;
    return false;
}

function resetAuthState() {
    try {
        fs.rmSync(AUTH_DIR, { recursive: true, force: true });
        fs.mkdirSync(AUTH_DIR, { recursive: true });
        return null;
    } catch (error) {
        return error;
    }
}

function shouldRetryTerminalRelink() {
    if (reconnectAttempt >= TERMINAL_RELINK_MAX_RETRIES) {
        return false;
    }
    reconnectAttempt += 1;
    return true;
}

function pruneOutboundMessageIds(now = Date.now()) {
    for (const [messageId, entry] of outboundMessageIds.entries()) {
        const ts = typeof entry === 'number' ? entry : entry?.ts;
        if (!Number.isFinite(ts) || (now - ts) > OUTBOUND_ECHO_TTL_MS) {
            outboundMessageIds.delete(messageId);
        }
    }
}

function rememberOutboundMessageId(messageId, jid) {
    if (typeof messageId !== 'string' || !messageId.trim()) return;
    const now = Date.now();
    pruneOutboundMessageIds(now);
    outboundMessageIds.set(messageId, {
        ts: now,
        jid: typeof jid === 'string' ? jid : null,
    });
    if (outboundMessageIds.size > 500) {
        const oldest = outboundMessageIds.keys().next().value;
        if (oldest) outboundMessageIds.delete(oldest);
    }
}

function isRecentOutboundMessageId(messageId) {
    if (typeof messageId !== 'string' || !messageId.trim()) return false;
    pruneOutboundMessageIds(Date.now());
    return outboundMessageIds.has(messageId);
}

function ownPhoneDigits() {
    return normalizeIdentifier(
        typeof sock?.user?.id === 'string'
            ? sock.user.id.split(':')[0]
            : (sock?.user?.phoneNumber || '')
    );
}

function ownExactJidCandidates() {
    const raw = [sock?.user?.lid, sock?.user?.id];
    const targets = [];
    for (const candidate of raw) {
        if (typeof candidate !== 'string') continue;
        const trimmed = candidate.trim();
        if (!trimmed) continue;
        if (!targets.includes(trimmed)) targets.push(trimmed);
    }
    return targets;
}

function pushUniqueTarget(targets, value) {
    if (typeof value !== 'string') return;
    const trimmed = value.trim();
    if (!trimmed) return;
    if (!targets.includes(trimmed)) {
        targets.push(trimmed);
    }
}

function resolveSendJidCandidates(jid) {
    const requested = typeof jid === 'string' ? jid.trim() : '';
    if (!requested) return [];

    const targets = [];
    const requestedUser = normalizeIdentifier(requested);
    const ownIds = collectOwnIdentifiers();
    const ownPhone = ownPhoneDigits();
    const ownExactJids = ownExactJidCandidates();
    const isSelfTarget = requestedUser && ownIds.has(requestedUser);

    if (isSelfTarget) {
        // Prefer exact logged-in identity JIDs (including device suffix) first.
        for (const ownJid of ownExactJids) {
            pushUniqueTarget(targets, ownJid);
        }
    }

    // Then try exactly what inbound channel carried.
    pushUniqueTarget(targets, requested);

    if (isSelfTarget && ownPhone) {
        // Canonical PN fallback still useful for some linked-account setups.
        pushUniqueTarget(targets, `${ownPhone}@s.whatsapp.net`);
    }

    if (requested.endsWith('@lid')) {
        const lidUser = normalizeJidUser(requested);
        if (/^\d{6,}$/.test(lidUser)) {
            pushUniqueTarget(targets, `${lidUser}@s.whatsapp.net`);
        }
    }

    if (requested.endsWith('@s.whatsapp.net')) {
        const pnUser = normalizeJidUser(requested);
        if (/^\d{6,}$/.test(pnUser)) {
            pushUniqueTarget(targets, `${pnUser}@lid`);
        }
    }

    return targets;
}

async function getBaileysApi() {
    if (baileysApi) return baileysApi;
    const mod = await import('@whiskeysockets/baileys');
    baileysApi = {
        makeWASocket: mod.default,
        fetchLatestBaileysVersion: mod.fetchLatestBaileysVersion,
        DisconnectReason: mod.DisconnectReason,
        useMultiFileAuthState: mod.useMultiFileAuthState,
        makeCacheableSignalKeyStore: mod.makeCacheableSignalKeyStore,
        Browsers: mod.Browsers,
    };
    return baileysApi;
}

// ---------------------------------------------------------------------------
// JSON-RPC communication
// ---------------------------------------------------------------------------

function sendEvent(event, data) {
    const msg = JSON.stringify({ event, data });
    protocolStdoutWrite(msg + '\n');
}

function emitTrace(phase, extra = {}) {
    sendEvent('trace', { phase, ...extra });
}

function summarizeReason(value) {
    if (typeof value !== 'string') return null;
    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : null;
}

function unwrapMessageContent(message) {
    let current = message || null;
    for (let i = 0; i < 8 && current; i += 1) {
        if (current.ephemeralMessage?.message) {
            current = current.ephemeralMessage.message;
            continue;
        }
        if (current.viewOnceMessage?.message) {
            current = current.viewOnceMessage.message;
            continue;
        }
        if (current.viewOnceMessageV2?.message) {
            current = current.viewOnceMessageV2.message;
            continue;
        }
        if (current.viewOnceMessageV2Extension?.message) {
            current = current.viewOnceMessageV2Extension.message;
            continue;
        }
        if (current.documentWithCaptionMessage?.message) {
            current = current.documentWithCaptionMessage.message;
            continue;
        }
        break;
    }
    return current || {};
}

function extractMessageText(message) {
    const content = unwrapMessageContent(message);
    return (
        content?.conversation
        || content?.extendedTextMessage?.text
        || content?.imageMessage?.caption
        || content?.videoMessage?.caption
        || content?.buttonsResponseMessage?.selectedDisplayText
        || content?.listResponseMessage?.title
        || content?.templateButtonReplyMessage?.selectedDisplayText
        || ''
    );
}

function normalizeJidUser(jid) {
    if (typeof jid !== 'string') return '';
    const atIdx = jid.indexOf('@');
    const withDevice = atIdx === -1 ? jid : jid.slice(0, atIdx);
    const colonIdx = withDevice.indexOf(':');
    return (colonIdx === -1 ? withDevice : withDevice.slice(0, colonIdx)).trim();
}

function normalizeIdentifier(value) {
    if (typeof value !== 'string') return '';
    const trimmed = value.trim();
    if (!trimmed) return '';
    const jidUser = normalizeJidUser(trimmed);
    if (jidUser) return jidUser.replace(/^\+/, '');
    return trimmed.replace(/^\+/, '');
}

function collectOwnIdentifiers() {
    const ids = new Set();
    const user = sock?.user || {};
    const candidates = [
        user?.id,
        user?.lid,
        user?.phoneNumber,
    ];
    for (const candidate of candidates) {
        const normalized = normalizeIdentifier(candidate);
        if (normalized) ids.add(normalized);
    }
    const idPhone = normalizeIdentifier(typeof user?.id === 'string' ? user.id.split(':')[0] : '');
    if (idPhone) ids.add(idPhone);
    return ids;
}

function isSelfChatRemoteJid(remoteJid, participantJid) {
    const ownIds = collectOwnIdentifiers();
    if (ownIds.size === 0) return false;
    const remoteUser = normalizeIdentifier(remoteJid);
    const participantUser = normalizeIdentifier(participantJid);
    if (remoteUser && ownIds.has(remoteUser)) return true;
    if (participantUser && ownIds.has(participantUser) && remoteUser && ownIds.has(remoteUser)) {
        return true;
    }
    return false;
}

function sendResult(id, result) {
    const msg = JSON.stringify({ id, result });
    protocolStdoutWrite(msg + '\n');
}

function sendError(id, error) {
    const msg = JSON.stringify({ id, error });
    protocolStdoutWrite(msg + '\n');
}

// ---------------------------------------------------------------------------
// WhatsApp connection
// ---------------------------------------------------------------------------

async function connectWhatsApp() {
    if (sock) {
        emitTrace('connect_ignored_already_active', {
            is_connected: isConnected,
            connect_attempt: connectAttempt,
        });
        return;
    }
    clearReconnectTimer();
    connectAttempt += 1;
    emitTrace('connect_attempt', {
        connect_attempt: connectAttempt,
        relink_retry_attempt: reconnectAttempt,
    });
    const {
        makeWASocket,
        Browsers,
        fetchLatestBaileysVersion,
        DisconnectReason,
        useMultiFileAuthState,
        makeCacheableSignalKeyStore,
    } = await getBaileysApi();
    const { state, saveCreds } = await useMultiFileAuthState(AUTH_DIR);
    const { version } = await fetchLatestBaileysVersion();
    emitTrace('baileys_version', { version, connect_attempt: connectAttempt });

    sock = makeWASocket({
        version,
        auth: {
            creds: state.creds,
            keys: makeCacheableSignalKeyStore(state.keys, logger),
        },
        printQRInTerminal: false,
        logger,
        browser: Browsers.ubuntu('Chrome'),
        generateHighQualityLinkPreview: false,
    });

    sock.ev.on('creds.update', saveCreds);

    sock.ev.on('connection.update', async (update) => {
        const { connection, lastDisconnect, qr } = update;
        emitTrace('connection_update', {
            connection: connection || null,
            connect_attempt: connectAttempt,
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
                    connect_attempt: connectAttempt,
                });
                emitTrace('qr_generated', {
                    connect_attempt: connectAttempt,
                    ascii_len: asciiQr.length,
                    has_data_url: true,
                });
            } catch (err) {
                sendEvent('error', `QR generation failed: ${err.message}`);
                emitTrace('qr_generation_failed', {
                    connect_attempt: connectAttempt,
                    error: err.message || String(err),
                });
            }
        }

        if (connection === 'close') {
            isConnected = false;
            sock = null;
            const statusCode = (lastDisconnect?.error)?.output?.statusCode;
            const numericStatusCode = Number.isFinite(statusCode) ? statusCode : null;
            const reconnectReason = summarizeReason(
                lastDisconnect?.error?.message ||
                lastDisconnect?.error?.toString?.() ||
                null
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
                connect_attempt: connectAttempt,
            });

            if (!terminalDisconnect) {
                sendEvent('reconnecting', {
                    reason: reconnectReason,
                    status_code: numericStatusCode,
                    relink_retry_attempt: reconnectAttempt + 1,
                    connect_attempt: connectAttempt,
                });
                scheduleReconnect();
            } else {
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
                        connect_attempt: connectAttempt,
                    });
                    return;
                }
                if (shouldRetryTerminalRelink()) {
                    sendEvent('reconnecting', {
                        reason: reconnectReason || 'terminal_relink_retry',
                        status_code: numericStatusCode,
                        relink_retry_attempt: reconnectAttempt,
                        connect_attempt: connectAttempt,
                    });
                    emitTrace('terminal_relink_retry', {
                        status_code: numericStatusCode,
                        reason: reconnectReason,
                        relink_retry_attempt: reconnectAttempt,
                        connect_attempt: connectAttempt,
                    });
                    scheduleReconnect();
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
                    connect_attempt: connectAttempt,
                });
            }
        } else if (connection === 'open') {
                clearReconnectTimer();
                reconnectAttempt = 0;
                isConnected = true;
                emitTrace('connected', {
                    connect_attempt: connectAttempt,
                    user_id: sock.user?.id || null,
                    user_lid: sock.user?.lid || null,
                });
                const phoneNumber = sock.user?.id?.split(':')[0] || 'Unknown';
            sendEvent('connected', { phone: `+${phoneNumber}` });
        }
    });

    sock.ev.on('messages.upsert', (m) => {
        emitTrace('messages_upsert_received', {
            upsert_type: m?.type || null,
            count: Array.isArray(m?.messages) ? m.messages.length : 0,
        });
        if (m.type !== 'notify' && m.type !== 'append') {
            emitTrace('messages_upsert_skipped_type', {
                upsert_type: m?.type || null,
                count: Array.isArray(m?.messages) ? m.messages.length : 0,
            });
            return;
        }
        for (const msg of m.messages) {
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
                    continue; // skip own messages except operator self-chat
                }
                emitTrace('message_from_me_self_chat_allowed', {
                    from,
                    participant: participantJid,
                    message_id: messageId,
                });
            }
            if (!from || from === 'status@broadcast') continue;
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

    sock.ev.on('messages.update', (updates) => {
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

    sock.ev.on('message-receipt.update', (updates) => {
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
    reconnectAttempt = 0;
    connectAttempt = 0;
    if (sock) {
        await sock.logout().catch(() => {});
        sock = null;
        isConnected = false;
    }
    emitTrace('manual_disconnect', {});
}

function getStatus() {
    if (!sock) return { status: 'disconnected', phone: null };
    if (isConnected) {
        const phoneNumber = sock.user?.id?.split(':')[0] || null;
        return { status: 'connected', phone: phoneNumber ? `+${phoneNumber}` : null };
    }
    return { status: 'connecting', phone: null };
}

async function sendWhatsAppMessage(jid, text) {
    if (!sock || !isConnected) {
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
    if (typeof sock.assertSessions === 'function') {
        try {
            const asserted = await sock.assertSessions(targets, true);
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
            const response = await sock.sendMessage(target, { text });
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

// ---------------------------------------------------------------------------
// stdin command handler
// ---------------------------------------------------------------------------

let inputBuffer = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', (chunk) => {
    inputBuffer += chunk;
    const lines = inputBuffer.split('\n');
    inputBuffer = lines.pop() || '';

    for (const line of lines) {
        if (!line.trim()) continue;
        try {
            const msg = JSON.parse(line);
            handleCommand(msg);
        } catch (err) {
            sendEvent('error', `Invalid JSON: ${err.message}`);
        }
    }
});

async function handleCommand(msg) {
    const { id, method, params } = msg;

    try {
        switch (method) {
            case 'connect':
                sendResult(id, 'ok');
                if (sock) {
                    emitTrace('connect_command_ignored', {
                        is_connected: isConnected,
                        connect_attempt: connectAttempt,
                    });
                    break;
                }
                // Reply immediately so the RPC never times out.
                // QR / connected / error events arrive asynchronously.
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

// Graceful shutdown
process.on('SIGTERM', async () => {
    clearReconnectTimer();
    if (sock) await sock.end(undefined).catch(() => {});
    process.exit(0);
});

process.on('SIGINT', async () => {
    clearReconnectTimer();
    if (sock) await sock.end(undefined).catch(() => {});
    process.exit(0);
});

sendEvent('ready', null);
