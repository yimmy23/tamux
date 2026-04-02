const path = require('path');
const fs = require('fs');
const os = require('os');
const util = require('util');
const pino = require('pino');

const AUTH_DIR = path.join(
    process.platform === 'win32' && process.env.LOCALAPPDATA
        ? path.join(process.env.LOCALAPPDATA, 'tamux')
        : path.join(os.homedir(), '.tamux'),
    'whatsapp-auth'
);
fs.mkdirSync(AUTH_DIR, { recursive: true });

const logger = pino({ level: 'silent' });
const protocolStdoutWrite = process.stdout.write.bind(process.stdout);

const state = {
    sock: null,
    isConnected: false,
    baileysApi: null,
    reconnectTimer: null,
    reconnectAttempt: 0,
    connectAttempt: 0,
    closedSessionRecoveryInFlight: false,
    stdoutGuardsInstalled: false,
    outboundMessageIds: new Map(),
};

const TERMINAL_RELINK_MAX_RETRIES = 1;
const OUTBOUND_ECHO_TTL_MS = 5 * 60 * 1000;

function relayConsoleToStderr(method, args, onClosedSessionWarning) {
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
        onClosedSessionWarning?.().catch(() => {});
    }
}

function installStdoutGuards({ onClosedSessionWarning }) {
    if (state.stdoutGuardsInstalled) {
        return;
    }

    for (const method of ['log', 'info', 'warn', 'error', 'debug']) {
        console[method] = (...args) =>
            relayConsoleToStderr(method, args, onClosedSessionWarning);
    }

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

    state.stdoutGuardsInstalled = true;
}

function clearReconnectTimer() {
    if (state.reconnectTimer) {
        clearTimeout(state.reconnectTimer);
        state.reconnectTimer = null;
    }
}

function scheduleReconnect(connectFn, sendEvent) {
    if (state.reconnectTimer) {
        return;
    }
    state.reconnectTimer = setTimeout(() => {
        state.reconnectTimer = null;
        connectFn().catch((err) => {
            sendEvent('error', `Reconnect failed: ${err.message || String(err)}`);
        });
    }, 3000);
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
    if (state.reconnectAttempt >= TERMINAL_RELINK_MAX_RETRIES) {
        return false;
    }
    state.reconnectAttempt += 1;
    return true;
}

function pruneOutboundMessageIds(now = Date.now()) {
    for (const [messageId, entry] of state.outboundMessageIds.entries()) {
        const ts = typeof entry === 'number' ? entry : entry?.ts;
        if (!Number.isFinite(ts) || (now - ts) > OUTBOUND_ECHO_TTL_MS) {
            state.outboundMessageIds.delete(messageId);
        }
    }
}

function rememberOutboundMessageId(messageId, jid) {
    if (typeof messageId !== 'string' || !messageId.trim()) return;
    const now = Date.now();
    pruneOutboundMessageIds(now);
    state.outboundMessageIds.set(messageId, {
        ts: now,
        jid: typeof jid === 'string' ? jid : null,
    });
    if (state.outboundMessageIds.size > 500) {
        const oldest = state.outboundMessageIds.keys().next().value;
        if (oldest) {
            state.outboundMessageIds.delete(oldest);
        }
    }
}

function isRecentOutboundMessageId(messageId) {
    if (typeof messageId !== 'string' || !messageId.trim()) return false;
    pruneOutboundMessageIds(Date.now());
    return state.outboundMessageIds.has(messageId);
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

function ownPhoneDigits() {
    return normalizeIdentifier(
        typeof state.sock?.user?.id === 'string'
            ? state.sock.user.id.split(':')[0]
            : (state.sock?.user?.phoneNumber || '')
    );
}

function ownExactJidCandidates() {
    const raw = [state.sock?.user?.lid, state.sock?.user?.id];
    const targets = [];
    for (const candidate of raw) {
        if (typeof candidate !== 'string') continue;
        const trimmed = candidate.trim();
        if (!trimmed) continue;
        if (!targets.includes(trimmed)) {
            targets.push(trimmed);
        }
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

function collectOwnIdentifiers() {
    const ids = new Set();
    const user = state.sock?.user || {};
    const candidates = [user?.id, user?.lid, user?.phoneNumber];
    for (const candidate of candidates) {
        const normalized = normalizeIdentifier(candidate);
        if (normalized) ids.add(normalized);
    }
    const idPhone = normalizeIdentifier(
        typeof user?.id === 'string' ? user.id.split(':')[0] : ''
    );
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
        for (const ownJid of ownExactJids) {
            pushUniqueTarget(targets, ownJid);
        }
    }

    pushUniqueTarget(targets, requested);

    if (isSelfTarget && ownPhone) {
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
    if (state.baileysApi) {
        return state.baileysApi;
    }
    const mod = await import('@whiskeysockets/baileys');
    state.baileysApi = {
        makeWASocket: mod.default,
        fetchLatestBaileysVersion: mod.fetchLatestBaileysVersion,
        DisconnectReason: mod.DisconnectReason,
        useMultiFileAuthState: mod.useMultiFileAuthState,
        makeCacheableSignalKeyStore: mod.makeCacheableSignalKeyStore,
        Browsers: mod.Browsers,
    };
    return state.baileysApi;
}

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

function sendResult(id, result) {
    const msg = JSON.stringify({ id, result });
    protocolStdoutWrite(msg + '\n');
}

function sendError(id, error) {
    const msg = JSON.stringify({ id, error });
    protocolStdoutWrite(msg + '\n');
}

module.exports = {
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
    normalizeJidUser,
    normalizeIdentifier,
    collectOwnIdentifiers,
    isSelfChatRemoteJid,
    sendResult,
    sendError,
};
