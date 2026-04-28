/**
 * WhatsApp Web bridge sidecar for zorai-gateway.
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

const { sendEvent } = require('./whatsapp-bridge/core.cjs');
const { createBridgeRuntime } = require('./whatsapp-bridge/runtime.cjs');

const runtime = createBridgeRuntime();

let inputBuffer = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', (chunk) => {
    inputBuffer += chunk;
    const lines = inputBuffer.split('\n');
    inputBuffer = lines.pop() || '';

    for (const line of lines) {
        if (!line.trim()) {
            continue;
        }
        try {
            const msg = JSON.parse(line);
            void runtime.handleCommand(msg);
        } catch (err) {
            sendEvent('error', `Invalid JSON: ${err.message}`);
        }
    }
});

async function shutdownAndExit() {
    await runtime.shutdown();
    process.exit(0);
}

process.on('SIGTERM', () => {
    void shutdownAndExit();
});

process.on('SIGINT', () => {
    void shutdownAndExit();
});

sendEvent('ready', null);
