const { app, BrowserWindow, Menu, clipboard, ipcMain, screen, shell, session } = require('electron');
const { spawn, spawnSync } = require('child_process');
const path = require('path');
const net = require('net');
const fs = require('fs');
const os = require('os');
const { pathToFileURL } = require('url');
const {
    createOpenAICodexAuthHandlers,
    pendingHandlerMatchesResponseType,
    resolvePendingAgentQueryEvent,
} = require('./agent-query-runtime.cjs');
const {
    configureChromiumRuntimePaths,
    deleteDataPath,
    ensureTamuxDataDir,
    getTamuxDataDir,
    logToFile,
    openDataPath,
    readJsonFile,
    readTextFile,
    revealDataPath,
    saveVisionScreenshot,
    writeJsonFile,
    writeTextFile,
    listDataDir,
} = require('./main/app-data.cjs');
const {
    discoverAITraining,
    discoverCodingAgents,
    resolveExecutablePath,
} = require('./main/agent-runtime-discovery.cjs');
const { cleanupDiscordClient, sendDiscordMessage } = require('./main/discord.cjs');
const {
    copyFsPath,
    createFsDirectory,
    deleteFsPath,
    getFsPathInfo,
    gitDiff,
    gitStatus,
    listFsDir,
    moveFsPath,
    readFsText,
    resolveFsPath,
    writeFsText,
} = require('./main/fs-git.cjs');
const { listInstalledPlugins, loadInstalledPluginScripts } = require('./main/plugins.cjs');
const { getAvailableShells, getSystemFonts, getSystemMonitorSnapshot } = require('./main/system-runtime.cjs');
const { createTerminalBridgeRuntime } = require('./main/terminal-bridge-runtime.cjs');

const DAEMON_NAME = 'tamux-daemon';
const CLI_NAME = 'tamux';
const DAEMON_TCP_HOST = '127.0.0.1';
const DAEMON_TCP_PORT = 17563;
const CLONE_SESSION_PREFIX = 'clone:';
const MAX_TERMINAL_HISTORY_BYTES = 1024 * 1024;
const MAX_REATTACH_HISTORY_BYTES = 64 * 1024;
const VISION_SCREENSHOT_TTL_MS = 10 * 60 * 1000;
let mainWindow = null;
let agentBridge = null;
let dbBridge = null;
// Module-level reference to sendAgentCommand (set during registerIpcHandlers)
let sendAgentCommandFn = null;

// ---------------------------------------------------------------------------
// WhatsApp bridge sidecar management (legacy fallback only)
// ---------------------------------------------------------------------------
let whatsappProcess = null;
let whatsappRpcId = 0;
const whatsappPendingCalls = new Map();
let whatsappDaemonSubscribed = false;
let whatsappDaemonSubscriptionDesired = false;
let agentBridgeRestartCooldownUntil = 0;
let agentBridgeConsecutiveExitCount = 0;
let desktopWhatsAppLinkingHelpersPromise = null;
const terminalBridgeRuntime = createTerminalBridgeRuntime({
    cloneSessionPrefix: CLONE_SESSION_PREFIX,
    fs,
    getCliPath,
    getMainWindow: () => mainWindow,
    logToFile,
    maxReattachHistoryBytes: MAX_REATTACH_HISTORY_BYTES,
    maxTerminalHistoryBytes: MAX_TERMINAL_HISTORY_BYTES,
    path,
    spawn,
    spawnDaemon,
});

function startWhatsAppBridge() {
    if (whatsappProcess) return;

    const bridgePath = path.join(__dirname, 'whatsapp-bridge.cjs');
    if (!fs.existsSync(bridgePath)) {
        logToFile('warn', 'whatsapp-bridge.cjs not found');
        throw new Error('WhatsApp bridge script not found');
    }

    logToFile('info', 'starting WhatsApp bridge sidecar');
    whatsappProcess = spawn(process.execPath, [bridgePath], {
        stdio: ['pipe', 'pipe', 'pipe'],
        env: { ...process.env, ELECTRON_RUN_AS_NODE: '1' },
    });

    let buffer = '';
    whatsappProcess.stdout.on('data', (chunk) => {
        buffer += chunk.toString();
        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        for (const line of lines) {
            if (!line.trim()) continue;
            try {
                const msg = JSON.parse(line);
                Promise.resolve(handleWhatsAppMessage(msg)).catch((err) => {
                    logToFile('warn', `WhatsApp bridge message handling failed: ${err.message || String(err)}`);
                });
            } catch (err) {
                logToFile('warn', `WhatsApp bridge invalid JSON: ${err.message}`);
            }
        }
    });

    whatsappProcess.stderr.on('data', (chunk) => {
        const text = chunk.toString().trim();
        logToFile('warn', `WhatsApp bridge stderr: ${text}`);
        // Forward critical errors to the UI so the user sees why it fails
        if (mainWindow && text) {
            mainWindow.webContents.send('whatsapp-error', `Bridge error: ${text}`);
        }
    });

    whatsappProcess.on('close', (code) => {
        logToFile('info', `WhatsApp bridge exited with code ${code}`);
        whatsappProcess = null;
        // Reject pending calls
        for (const [, { reject }] of whatsappPendingCalls) {
            reject(new Error('WhatsApp bridge process exited'));
        }
        whatsappPendingCalls.clear();
        // Notify UI that bridge died
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

async function handleWhatsAppMessage(msg) {
    // RPC response
    if (msg.id !== undefined && whatsappPendingCalls.has(msg.id)) {
        const { resolve, reject } = whatsappPendingCalls.get(msg.id);
        whatsappPendingCalls.delete(msg.id);
        if (msg.error) reject(new Error(msg.error));
        else resolve(msg.result);
        return;
    }

    // Event from bridge
    if (msg.event && mainWindow) {
        switch (msg.event) {
            case 'qr':
                logToFile('info', 'WhatsApp bridge qr event', {
                    asciiLen: typeof msg.data?.ascii_qr === 'string' ? msg.data.ascii_qr.length : 0,
                    hasDataUrl: typeof msg.data?.data_url === 'string',
                });
                {
                    const { getRendererWhatsAppQrDataUrl } = await getDesktopWhatsAppLinkingHelpers();
                    mainWindow.webContents.send('whatsapp-qr', getRendererWhatsAppQrDataUrl(msg.data));
                }
                break;
            case 'connected':
                logToFile('info', 'WhatsApp bridge connected event', {
                    phone: msg.data?.phone || null,
                });
                mainWindow.webContents.send('whatsapp-connected', msg.data);
                break;
            case 'disconnected':
                logToFile('info', 'WhatsApp bridge disconnected event', {
                    reason: msg.data?.reason || null,
                });
                mainWindow.webContents.send('whatsapp-disconnected', {
                    reason: msg.data?.reason || null,
                });
                break;
            case 'error':
                logToFile('warn', 'WhatsApp bridge error event', {
                    message: msg.data || null,
                });
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
}

function whatsappRpc(method, params) {
    return new Promise((resolve, reject) => {
        if (!whatsappProcess) {
            reject(new Error('WhatsApp bridge not running'));
            return;
        }
        const id = ++whatsappRpcId;
        whatsappPendingCalls.set(id, { resolve, reject });
        const msg = JSON.stringify({ id, method, params }) + '\n';
        whatsappProcess.stdin.write(msg);

        // Timeout after 60 seconds (Baileys connection can be slow)
        setTimeout(() => {
            if (whatsappPendingCalls.has(id)) {
                whatsappPendingCalls.delete(id);
                reject(new Error('WhatsApp RPC timeout'));
            }
        }, 60000);
    });
}

function stopWhatsAppBridge() {
    if (whatsappProcess) {
        whatsappProcess.kill('SIGTERM');
        whatsappProcess = null;
    }
}

async function convertWhatsAppQrToDataUrl(qrPayload) {
    if (typeof qrPayload !== 'string' || !qrPayload.trim()) {
        throw new Error('WhatsApp QR payload is empty');
    }
    if (qrPayload.startsWith('data:image/')) {
        return qrPayload;
    }
    const qrcode = require('qrcode');
    return await qrcode.toDataURL(qrPayload, { margin: 1, scale: 6 });
}

function getDaemonPath() {
    const resolved = getCompanionBinaryPath(DAEMON_NAME);
    logToFile('info', 'resolved daemon path', { resolved });
    return resolved;
}

function getCliPath() {
    const resolved = getCompanionBinaryPath(CLI_NAME);
    logToFile('info', 'resolved cli path', { resolved });
    return resolved;
}

function getCompanionBinaryPath(binaryName) {
    const isDev = !app.isPackaged;
    const exeName = binaryName + (process.platform === 'win32' ? '.exe' : '');

    if (isDev) {
        const repoRoot = path.join(__dirname, '..', '..');
        const candidates = [
            path.join(repoRoot, 'target', 'debug', exeName),
            path.join(repoRoot, 'target', 'release', exeName),
            path.join(repoRoot, 'dist', exeName),
            path.join(repoRoot, 'target', 'x86_64-pc-windows-gnu', 'release', exeName),
        ];

        const existing = candidates.find((candidate) => fs.existsSync(candidate));
        return existing || candidates[0];
    }

    const exeDir = path.dirname(app.getPath('exe'));
    const resourceDir = process.resourcesPath;
    const packagedCandidates = [
        path.join(resourceDir, 'bin', exeName),
        path.join(resourceDir, exeName),
        path.join(resourceDir, 'dist', exeName),
        path.join(resourceDir, 'app.asar.unpacked', 'dist', exeName),
        path.join(exeDir, exeName),
    ];
    const existing = packagedCandidates.find((candidate) => fs.existsSync(candidate));
    return existing || packagedCandidates[0];
}

function sendAppCommand(command) {
    mainWindow?.webContents.send('app-command', command);
}

function buildAppMenu() {
    const template = [
        {
            label: 'File',
            submenu: [
                { label: 'New Workspace', accelerator: 'Ctrl+Shift+N', click: () => sendAppCommand('new-workspace') },
                { label: 'New Surface', accelerator: 'Ctrl+T', click: () => sendAppCommand('new-surface') },
                { type: 'separator' },
                { label: 'Settings', accelerator: 'Ctrl+,', click: () => sendAppCommand('toggle-settings') },
                { type: 'separator' },
                { role: 'quit', label: 'Exit' },
            ],
        },
        {
            label: 'Edit',
            submenu: [
                { role: 'undo' },
                { role: 'redo' },
                { type: 'separator' },
                { role: 'cut' },
                {
                    label: 'Copy',
                    accelerator: 'Ctrl+C',
                    click: () => {
                        mainWindow?.webContents.copy();
                        sendAppCommand('copy');
                    },
                },
                {
                    label: 'Paste',
                    accelerator: 'Ctrl+V',
                    click: () => {
                        mainWindow?.webContents.paste();
                        sendAppCommand('paste');
                    },
                },
                {
                    label: 'Select All',
                    accelerator: 'Ctrl+A',
                    click: () => {
                        mainWindow?.webContents.selectAll();
                        sendAppCommand('select-all');
                    },
                },
            ],
        },
        {
            label: 'View',
            submenu: [
                { label: 'Command Palette', accelerator: 'Ctrl+Shift+P', click: () => sendAppCommand('toggle-command-palette') },
                { label: 'Search', accelerator: 'Ctrl+Shift+F', click: () => sendAppCommand('toggle-search') },
                { label: 'File Manager', accelerator: 'Ctrl+Shift+E', click: () => sendAppCommand('toggle-file-manager') },
                { label: 'Toggle Sidebar', accelerator: 'Ctrl+B', click: () => sendAppCommand('toggle-sidebar') },
                { type: 'separator' },
                { role: 'reload' },
                { role: 'forceReload' },
                { role: 'toggleDevTools' },
                { type: 'separator' },
                { role: 'resetZoom' },
                { role: 'zoomIn' },
                { role: 'zoomOut' },
                { role: 'togglefullscreen' },
            ],
        },
        {
            label: 'Features',
            submenu: [
                { label: 'Mission Console', click: () => sendAppCommand('toggle-mission') },
                { label: 'Command History', click: () => sendAppCommand('toggle-command-history') },
                { label: 'Command Log', click: () => sendAppCommand('toggle-command-log') },
                { label: 'Session Vault', click: () => sendAppCommand('toggle-session-vault') },
                { label: 'System Monitor', click: () => sendAppCommand('toggle-system-monitor') },
                { label: 'Execution Canvas', click: () => sendAppCommand('toggle-canvas') },
                { label: 'Time Travel Snapshots', click: () => sendAppCommand('toggle-time-travel') },
            ],
        },
        {
            label: 'Window',
            submenu: [
                { label: 'Split Right', accelerator: 'Ctrl+D', click: () => sendAppCommand('split-right') },
                { label: 'Split Down', accelerator: 'Ctrl+Shift+D', click: () => sendAppCommand('split-down') },
                { label: 'Zoom Pane', accelerator: 'Ctrl+Shift+Z', click: () => sendAppCommand('toggle-zoom') },
                { type: 'separator' },
                { role: 'minimize' },
                { role: 'close' },
            ],
        },
        {
            label: 'Help',
            submenu: [
                { label: 'About', click: () => sendAppCommand('about') },
            ],
        },
    ];

    return Menu.buildFromTemplate(template);
}

function setWindowOpacity(opacity) {
    const normalized = Number.isFinite(opacity) ? Math.min(1, Math.max(0.35, Number(opacity))) : 1;
    if (mainWindow && typeof mainWindow.setOpacity === 'function') {
        mainWindow.setOpacity(normalized);
    }
    return normalized;
}

function commandExists(binary) {
    if (typeof binary !== 'string' || !binary.trim()) return false;
    try {
        const checker = process.platform === 'win32' ? 'where' : 'which';
        const result = spawnSync(checker, [binary], { stdio: 'ignore' });
        return result.status === 0;
    } catch {
        return false;
    }
}

const SETUP_DEPENDENCY_DEFS = {
    cargo: { name: 'cargo', label: 'Rust cargo', command: 'cargo' },
    node: { name: 'node', label: 'Node.js', command: 'node' },
    npm: { name: 'npm', label: 'npm', command: 'npm' },
    git: { name: 'git', label: 'git', command: 'git' },
    uv: { name: 'uv', label: 'uv', command: 'uv' },
    aline: { name: 'aline', label: 'Aline CLI', command: 'aline' },
    'tamux-mcp': { name: 'tamux-mcp', label: 'tamux-mcp', command: 'tamux-mcp' },
    hermes: { name: 'hermes', label: 'Hermes Agent', command: 'hermes' },
    openclaw: { name: 'openclaw', label: 'OpenClaw', command: 'openclaw' },
};

const SETUP_REQUIRED_BY_PROFILE = {
    source: ['cargo', 'node', 'npm', 'git', 'uv'],
    desktop: [],
};

const SETUP_OPTIONAL = ['aline', 'tamux-mcp', 'hermes', 'openclaw'];

function setupInstallHint(depName) {
    const platform = process.platform;
    switch (depName) {
        case 'cargo':
            return platform === 'win32'
                ? ['winget install Rustlang.Rustup']
                : ['curl https://sh.rustup.rs -sSf | sh'];
        case 'node':
        case 'npm':
            if (platform === 'darwin') return ['brew install node'];
            if (platform === 'win32') return ['winget install OpenJS.NodeJS.LTS'];
            return ['sudo apt update && sudo apt install -y nodejs npm'];
        case 'git':
            if (platform === 'darwin') return ['brew install git'];
            if (platform === 'win32') return ['winget install Git.Git'];
            return ['sudo apt update && sudo apt install -y git'];
        case 'uv':
            if (platform === 'win32') {
                return ['powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"'];
            }
            return ['curl -LsSf https://astral.sh/uv/install.sh | sh'];
        case 'aline':
            return ['uv tool install aline-ai'];
        case 'tamux-mcp':
            return ['cargo build --release -p tamux-mcp'];
        case 'hermes':
            return ['python3 -m pip install "hermes-agent[all]"'];
        case 'openclaw':
            return ['npm install -g openclaw'];
        default:
            return [];
    }
}

function resolveGettingStartedPath() {
    const devCandidate = path.join(__dirname, '..', '..', 'docs', 'getting-started.md');
    const packagedCandidates = [
        path.join(process.resourcesPath, 'GETTING_STARTED.md'),
        path.join(process.resourcesPath, 'app.asar.unpacked', 'GETTING_STARTED.md'),
    ];
    for (const candidate of [devCandidate, ...packagedCandidates]) {
        if (fs.existsSync(candidate)) {
            return candidate;
        }
    }
    return devCandidate;
}

function collectSetupDependency(name) {
    const def = SETUP_DEPENDENCY_DEFS[name];
    if (!def) return null;
    const commandPath = resolveExecutablePath(def.command);
    return {
        name: def.name,
        label: def.label,
        command: def.command,
        found: Boolean(commandPath),
        path: commandPath,
        installHints: setupInstallHint(def.name),
    };
}

function checkSetupPrereqs(_event, profile = 'desktop') {
    const normalizedProfile = profile === 'source' ? 'source' : 'desktop';
    const requiredNames = SETUP_REQUIRED_BY_PROFILE[normalizedProfile] || SETUP_REQUIRED_BY_PROFILE.desktop;
    const required = requiredNames
        .map((name) => collectSetupDependency(name))
        .filter(Boolean);
    const optional = SETUP_OPTIONAL
        .map((name) => collectSetupDependency(name))
        .filter(Boolean);
    const missingRequired = required.filter((entry) => !entry.found).map((entry) => entry.name);
    const daemonPath = getDaemonPath();

    return {
        profile: normalizedProfile,
        platform: process.platform,
        required,
        optional,
        missingRequired,
        daemonPath,
        cliPath: getCliPath(),
        installRoot: path.dirname(daemonPath),
        dataDir: ensureTamuxDataDir(),
        gettingStartedPath: resolveGettingStartedPath(),
        whatIsTamux: 'tamux is an AI-native terminal multiplexer with a Rust daemon, pane/session control, and agent workflows.',
    };
}

function checkLspHealth() {
    return {
        rustAnalyzer: commandExists('rust-analyzer'),
        typescriptLanguageServer: commandExists('typescript-language-server'),
        pyrightLangserver: commandExists('pyright-langserver'),
    };
}

function checkMcpHealth(_event, servers = {}) {
    const checks = [];
    const entries = typeof servers === 'object' && servers !== null
        ? Object.entries(servers)
        : [];

    checks.push({
        name: 'tamux',
        command: 'tamux-mcp',
        exists: commandExists('tamux-mcp'),
    });

    for (const [name, value] of entries) {
        const command = typeof value?.command === 'string' ? value.command.trim() : '';
        if (!command) {
            checks.push({ name, command: '', exists: false, error: 'missing command' });
            continue;
        }
        checks.push({
            name,
            command,
            exists: commandExists(command),
        });
    }

    return checks;
}

function getDaemonEndpoint() {
    if (process.platform === 'win32') {
        return { host: DAEMON_TCP_HOST, port: DAEMON_TCP_PORT };
    }
    const runtimeDir = process.env.XDG_RUNTIME_DIR || '/tmp';
    return { path: path.join(runtimeDir, 'tamux-daemon.sock') };
}

async function checkDaemonRunning() {
    const endpoint = getDaemonEndpoint();
    logToFile('info', 'checking daemon endpoint', endpoint);
    return new Promise((resolve) => {
        const socket = new net.Socket();
        socket.setTimeout(1000);
        socket.once('connect', () => { socket.destroy(); resolve(true); });
        socket.once('error', () => { socket.destroy(); resolve(false); });
        socket.once('timeout', () => { socket.destroy(); resolve(false); });
        socket.connect(endpoint);
    });
}

async function spawnDaemon() {
    const isRunning = await checkDaemonRunning();
    if (isRunning) { console.log('[tamux] Daemon already running'); return true; }

    const daemonPath = getDaemonPath();
    console.log('[tamux] Spawning daemon:', daemonPath);
    logToFile('info', 'spawning daemon', { daemonPath });

    if (!fs.existsSync(daemonPath)) {
        console.error('[tamux] Daemon binary not found at:', daemonPath);
        logToFile('error', 'daemon binary missing', { daemonPath });
        return false;
    }

    const daemonEnv = { ...process.env };
    if (!daemonEnv.TAMUX_WHATSAPP_NODE_BIN || !String(daemonEnv.TAMUX_WHATSAPP_NODE_BIN).trim()) {
        daemonEnv.TAMUX_WHATSAPP_NODE_BIN = process.execPath;
    }
    if (!daemonEnv.TAMUX_WHATSAPP_BRIDGE_PATH || !String(daemonEnv.TAMUX_WHATSAPP_BRIDGE_PATH).trim()) {
        daemonEnv.TAMUX_WHATSAPP_BRIDGE_PATH = path.join(__dirname, 'whatsapp-bridge.cjs');
    }

    const daemon = spawn(daemonPath, [], {
        detached: true, stdio: 'ignore', windowsHide: true,
        cwd: path.dirname(daemonPath),
        env: daemonEnv,
    });
    daemon.on('error', (err) => {
        console.error('[tamux] Daemon error:', err);
        logToFile('error', 'daemon process error', { message: err.message });
    });
    daemon.unref();

    for (let i = 0; i < 20; i++) {
        await new Promise(r => setTimeout(r, 250));
        if (await checkDaemonRunning()) {
            console.log('[tamux] Daemon ready');
            logToFile('info', 'daemon ready');
            agentBridgeConsecutiveExitCount = 0;
            agentBridgeRestartCooldownUntil = 0;
            return true;
        }
    }
    console.warn('[tamux] Daemon did not become ready');
    logToFile('error', 'daemon did not become ready');
    return false;
}

function escapeJsonPointerSegment(segment) {
    return String(segment).replace(/~/g, "~0").replace(/\//g, "~1");
}

function flattenConfigEntries(value, pointer = "", entries = []) {
    if (value && typeof value === "object" && !Array.isArray(value)) {
        const keys = Object.keys(value);
        if (keys.length > 0) {
            for (const key of keys) {
                flattenConfigEntries(value[key], `${pointer}/${escapeJsonPointerSegment(key)}`, entries);
            }
            return entries;
        }
    }
    entries.push([pointer, value]);
    return entries;
}

function createWindow() {
    const { width: screenW, height: screenH } = screen.getPrimaryDisplay().workAreaSize;
    const useNativeFrame = process.platform === 'win32';

    mainWindow = new BrowserWindow({
        width: Math.min(1400, screenW), height: Math.min(900, screenH),
        minWidth: 600, minHeight: 400,
        frame: useNativeFrame,
        titleBarStyle: useNativeFrame ? 'default' : 'hidden',
        autoHideMenuBar: false,
        titleBarOverlay: useNativeFrame ? undefined : process.platform === 'win32' ? {
            color: '#181825', symbolColor: '#cdd6f4', height: 36,
        } : undefined,
        webPreferences: {
            preload: path.join(__dirname, 'preload.cjs'),
            nodeIntegration: false, contextIsolation: true,
            webviewTag: true,
        },
        title: 'tamux',
        icon: path.join(__dirname, '..', 'assets', 'icon.ico'),
        backgroundColor: '#1e1e2e', show: false,
        opacity: 1,
    });

    const isDev = !app.isPackaged;
    if (isDev) mainWindow.loadURL('http://localhost:5173');
    else mainWindow.loadFile(path.join(__dirname, '..', 'dist', 'index.html'));

    mainWindow.webContents.setWindowOpenHandler(({ url }) => {
        if (/^https?:\/\//i.test(url)) {
            void shell.openExternal(url);
            return { action: 'deny' };
        }

        return { action: 'allow' };
    });

    mainWindow.webContents.on('will-navigate', (event, url) => {
        const currentUrl = mainWindow?.webContents.getURL();
        if (url !== currentUrl && /^https?:\/\//i.test(url)) {
            event.preventDefault();
            void shell.openExternal(url);
        }
    });

    Menu.setApplicationMenu(buildAppMenu());

    mainWindow.once('ready-to-show', () => mainWindow.show());
    if (isDev) mainWindow.webContents.openDevTools();

    mainWindow.on('maximize', () => mainWindow.webContents.send('window-state', 'maximized'));
    mainWindow.on('unmaximize', () => mainWindow.webContents.send('window-state', 'normal'));
    mainWindow.on('closed', () => {
        logToFile('info', 'main window closed');
        terminalBridgeRuntime.stopAllTerminalBridges(true, true);
        mainWindow = null;
    });
}

function registerIpcHandlers() {
    const openAICodexAuthHandlers = createOpenAICodexAuthHandlers(sendAgentQuery);

    ipcMain.handle('getSocketPath', () => {
        const endpoint = getDaemonEndpoint();
        return endpoint.path ?? `${endpoint.host}:${endpoint.port}`;
    });
    ipcMain.handle('checkDaemon', () => checkDaemonRunning());
    ipcMain.handle('spawnDaemon', () => spawnDaemon());
    ipcMain.handle('getSystemFonts', () => getSystemFonts());
    ipcMain.handle('getAvailableShells', () => getAvailableShells());
    ipcMain.handle('system-monitor-snapshot', (_event, options) => getSystemMonitorSnapshot(options));
    ipcMain.handle('getDaemonPath', () => getDaemonPath());
    ipcMain.handle('getPlatform', () => process.platform);
    ipcMain.handle('setup-check-prereqs', (event, profile) => checkSetupPrereqs(event, profile));
    ipcMain.handle('coding-agents-discover', () => discoverCodingAgents());
    ipcMain.handle('ai-training-discover', (_event, workspacePath) => discoverAITraining(workspacePath));
    ipcMain.handle('plugin-list-installed', () => listInstalledPlugins());
    ipcMain.handle('plugin-load-installed', () => loadInstalledPluginScripts());

    // Plugin daemon IPC handlers (Plan 16-01)
    ipcMain.handle('plugin-daemon-list', async () => {
        try {
            return await sendAgentQuery({ type: 'plugin-list' }, 'plugin-list-result');
        } catch (err) {
            return { plugins: [] };
        }
    });
    ipcMain.handle('plugin-daemon-get', async (_event, name) => {
        try {
            return await sendAgentQuery({ type: 'plugin-get', name }, 'plugin-get-result');
        } catch (err) {
            return { plugin: null, settings_schema: null };
        }
    });
    ipcMain.handle('plugin-daemon-enable', async (_event, name) => {
        try {
            sendAgentCommand({ type: 'plugin-enable', name });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });
    ipcMain.handle('plugin-daemon-disable', async (_event, name) => {
        try {
            sendAgentCommand({ type: 'plugin-disable', name });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });
    ipcMain.handle('plugin-get-settings', async (_event, name) => {
        try {
            return await sendAgentQuery({ type: 'plugin-get-settings', name }, 'plugin-settings');
        } catch (err) {
            return { plugin_name: name, settings: [] };
        }
    });
    ipcMain.handle('plugin-update-settings', async (_event, pluginName, key, value, isSecret) => {
        try {
            sendAgentCommand({
                type: 'plugin-update-settings',
                plugin_name: pluginName,
                key,
                value: String(value),
                is_secret: Boolean(isSecret),
            });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });
    ipcMain.handle('plugin-test-connection', async (_event, name) => {
        try {
            return await sendAgentQuery({ type: 'plugin-test-connection', name }, 'plugin-test-connection-result');
        } catch (err) {
            return { plugin_name: name, success: false, message: err.message || 'Bridge error' };
        }
    });
    // Plugin OAuth start (Plan 18-03) — sends PluginOAuthStart to daemon, returns { name, url }
    ipcMain.handle('plugin-oauth-start', async (_event, name) => {
        try {
            const result = await sendAgentQuery({ type: 'plugin-oauth-start', name }, 'plugin-oauth-url', 30000);
            // Also open browser automatically
            if (result?.url) {
                shell.openExternal(result.url);
            }
            return { name: result?.name ?? name, url: result?.url ?? '' };
        } catch (err) {
            return { name, url: '', error: err.message || 'Failed to start OAuth flow' };
        }
    });
    ipcMain.handle('diagnostics-check-lsp', checkLspHealth);
    ipcMain.handle('diagnostics-check-mcp', checkMcpHealth);
    ipcMain.handle('persistence-get-data-dir', () => ensureTamuxDataDir());
    ipcMain.handle('persistence-read-json', (_event, relativePath) => readJsonFile(relativePath));
    ipcMain.handle('persistence-write-json', (_event, relativePath, data) => writeJsonFile(relativePath, data));
    ipcMain.handle('persistence-read-text', (_event, relativePath) => readTextFile(relativePath));
    ipcMain.handle('persistence-write-text', (_event, relativePath, content) => writeTextFile(relativePath, content));
    ipcMain.handle('persistence-delete-path', (_event, relativePath) => deleteDataPath(relativePath));
    ipcMain.handle('persistence-list-dir', (_event, relativeDir) => listDataDir(relativeDir));
    ipcMain.handle('persistence-open-path', (_event, relativePath) => openDataPath(relativePath, shell));
    ipcMain.handle('persistence-reveal-path', (_event, relativePath) => revealDataPath(relativePath, shell));
    ipcMain.handle('fs-list-dir', (_event, targetDir) => listFsDir(targetDir));
    ipcMain.handle('fs-copy-path', (_event, sourcePath, destinationPath) => copyFsPath(sourcePath, destinationPath));
    ipcMain.handle('fs-move-path', (_event, sourcePath, destinationPath) => moveFsPath(sourcePath, destinationPath));
    ipcMain.handle('fs-delete-path', (_event, targetPath) => deleteFsPath(targetPath));
    ipcMain.handle('fs-mkdir', (_event, targetDirPath) => createFsDirectory(targetDirPath));
    ipcMain.handle('fs-open-path', (_event, targetPath) => shell.openPath(resolveFsPath(targetPath)));
    ipcMain.handle('fs-reveal-path', (_event, targetPath) => {
        shell.showItemInFolder(resolveFsPath(targetPath));
        return true;
    });
    ipcMain.handle('fs-read-text', (_event, targetPath) => readFsText(targetPath));
    ipcMain.handle('fs-write-text', (_event, targetPath, content) => writeFsText(targetPath, content));
    ipcMain.handle('fs-path-info', (_event, targetPath) => getFsPathInfo(targetPath));
    ipcMain.handle('git-status', (_event, targetPath) => gitStatus(targetPath));
    ipcMain.handle('git-diff', (_event, targetPath, filePath) => gitDiff(targetPath, filePath));
    ipcMain.handle('clipboard-read-text', () => clipboard.readText());
    ipcMain.handle('clipboard-write-text', (_event, text) => {
        clipboard.writeText(typeof text === 'string' ? text : '');
        return true;
    });
    ipcMain.handle('terminal-start', terminalBridgeRuntime.startTerminalBridge);
    ipcMain.handle('terminal-input', terminalBridgeRuntime.sendTerminalInput);
    ipcMain.handle('terminal-execute-managed', terminalBridgeRuntime.executeManagedCommand);
    ipcMain.handle('terminal-approval-decision', terminalBridgeRuntime.resolveManagedApproval);
    ipcMain.handle('terminal-search-history', terminalBridgeRuntime.searchManagedHistory);
    ipcMain.handle('terminal-generate-skill', terminalBridgeRuntime.generateManagedSkill);
    ipcMain.handle('terminal-find-symbol', terminalBridgeRuntime.findManagedSymbol);
    ipcMain.handle('terminal-list-snapshots', terminalBridgeRuntime.listSnapshots);
    ipcMain.handle('terminal-restore-snapshot', terminalBridgeRuntime.restoreSnapshot);
    ipcMain.handle('terminal-clone-session', terminalBridgeRuntime.cloneTerminalSession);
    ipcMain.handle('terminal-resize', terminalBridgeRuntime.resizeTerminalSession);
    ipcMain.handle('terminal-stop', (_event, paneId, killSession) => terminalBridgeRuntime.stopTerminalBridge(paneId, Boolean(killSession)));
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
            await sendDbAckCommand({
                type: 'delete-agent-messages',
                thread_id: threadId,
                message_ids: [messageId],
            });
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
            return await sendDbQuery({
                type: 'list-transcript-index',
                workspace_id: typeof workspaceId === 'string' ? workspaceId : null,
            }, 'transcript-index-entries');
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
            return await sendDbQuery({
                type: 'list-snapshot-index',
                workspace_id: typeof workspaceId === 'string' ? workspaceId : null,
            }, 'snapshot-index-entries');
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
    ipcMain.handle('window-minimize', () => mainWindow?.minimize());
    ipcMain.handle('window-maximize', () => {
        if (mainWindow?.isMaximized()) mainWindow.unmaximize();
        else mainWindow?.maximize();
    });
    ipcMain.handle('window-close', () => {
        terminalBridgeRuntime.stopAllTerminalBridges(true, true);
        app.quit();
        return true;
    });
    ipcMain.handle('window-isMaximized', () => mainWindow?.isMaximized() ?? false);
    ipcMain.handle('window-set-opacity', (_event, opacity) => setWindowOpacity(opacity));
    ipcMain.handle('vision-save-screenshot', (_event, payload) => saveVisionScreenshot(payload, {
        ttlMs: VISION_SCREENSHOT_TTL_MS,
    }));
    ipcMain.handle('discord-send-message', (_event, payload) => sendDiscordMessage(payload));

    function isWhatsAppElectronFallbackEnabled(config) {
        return config?.gateway?.whatsapp_link_fallback_electron === true;
    }

    async function getGatewayConfigForWhatsAppFallback() {
        try {
            const config = await sendAgentQuery({ type: 'get-config' }, 'config');
            return config ?? null;
        } catch (error) {
            throw new Error(`Failed to read gateway config for WhatsApp fallback: ${error.message || String(error)}`);
        }
    }

    function getDesktopWhatsAppLinkingHelpers() {
        if (!desktopWhatsAppLinkingHelpersPromise) {
            desktopWhatsAppLinkingHelpersPromise = import(
                pathToFileURL(path.join(__dirname, 'whatsappLinking.js')).href
            );
        }
        return desktopWhatsAppLinkingHelpersPromise;
    }

    async function assertValidWhatsAppAllowlistForConnect(config) {
        const { assertValidWhatsAppConnectConfig } = await getDesktopWhatsAppLinkingHelpers();
        assertValidWhatsAppConnectConfig(config);
    }

    async function ensureDaemonWhatsAppSubscribed() {
        if (whatsappDaemonSubscribed) return;
        sendAgentCommand({ type: 'whats-app-link-subscribe' });
        whatsappDaemonSubscribed = true;
        whatsappDaemonSubscriptionDesired = true;
    }

    // WhatsApp link bridge: daemon protocol by default, Electron sidecar if fallback flag is enabled.
    ipcMain.handle('whatsapp-connect', async () => {
        try {
            const config = await getGatewayConfigForWhatsAppFallback();
            await assertValidWhatsAppAllowlistForConnect(config);
            if (isWhatsAppElectronFallbackEnabled(config)) {
                startWhatsAppBridge();
                await whatsappRpc('connect');
                return { ok: true };
            }
            await ensureDaemonWhatsAppSubscribed();
            sendAgentCommand({ type: 'whats-app-link-start' });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });
    ipcMain.handle('whatsapp-disconnect', async () => {
        try {
            const config = await getGatewayConfigForWhatsAppFallback();
            if (isWhatsAppElectronFallbackEnabled(config)) {
                if (whatsappProcess) {
                    await whatsappRpc('disconnect').catch(() => {});
                }
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
            const config = await getGatewayConfigForWhatsAppFallback();
            if (isWhatsAppElectronFallbackEnabled(config)) {
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
            return {
                status: 'error',
                phone: null,
                phoneNumber: null,
                lastError: error.message || String(error),
            };
        }
    });
    ipcMain.handle('whatsapp-send', async (_event, jid, text) => {
        const target = typeof jid === 'string' ? jid.trim() : '';
        const message = typeof text === 'string' ? text : '';
        if (!target || !message) {
            return { ok: false, error: 'whatsapp-send requires jid and text' };
        }
        return {
            ok: false,
            error: 'whatsapp-send is disabled in Electron; daemon gateway messaging is authoritative',
        };
    });

    // -----------------------------------------------------------------
    // Agent engine — bridge process IPC
    //
    // Spawns a persistent `tamux agent-bridge` CLI process that
    // maintains a socket connection to the daemon. All agent messages
    // flow through this bridge as JSON over stdin/stdout.
    // -----------------------------------------------------------------

    function ensureAgentBridge() {
        if (agentBridge && !agentBridge.process.killed) return agentBridge;
        const now = Date.now();
        if (now < agentBridgeRestartCooldownUntil) {
            return null;
        }

        const cliPath = getDaemonPath().replace(/tamux-daemon/, 'tamux').replace(/tamux-daemon\.exe/, 'tamux.exe');
        if (!fs.existsSync(cliPath)) {
            logToFile('warn', 'agent bridge: tamux CLI not found', { cliPath });
            return null;
        }

        const bridgeProcess = spawn(cliPath, ['agent-bridge'], {
            cwd: path.dirname(cliPath),
            windowsHide: true,
            stdio: ['pipe', 'pipe', 'pipe'],
        });

        agentBridge = {
            process: bridgeProcess,
            ready: false,
            pending: new Map(), // requestId -> { resolve, reject }
            stdoutBuffer: '',
        };

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
                    agentBridgeConsecutiveExitCount = 0;
                    agentBridgeRestartCooldownUntil = 0;
                    logToFile('info', 'agent bridge ready');
                    if (whatsappDaemonSubscriptionDesired && !whatsappDaemonSubscribed) {
                        try {
                            sendAgentCommand({ type: 'whats-app-link-subscribe' });
                            whatsappDaemonSubscribed = true;
                            logToFile('info', 'restored daemon WhatsApp subscription after bridge ready');
                        } catch (error) {
                            logToFile('warn', 'failed to restore daemon WhatsApp subscription', {
                                error: error?.message || String(error),
                            });
                        }
                    }
                    continue;
                }

                if (event.type === 'concierge_welcome') {
                    logToFile('info', '[concierge] received concierge_welcome from bridge', { hasContent: !!event.content, hasActions: !!event.actions });
                }

                // Response types from daemon queries — resolve oldest pending
                // request of the matching type (FIFO order).
                if (resolvePendingAgentQueryEvent(agentBridge, event)) {
                    continue;
                }

                if (event.type === 'error') {
                    // Match daemon query errors to the oldest pending request (FIFO),
                    // mirroring normal query response correlation behavior.
                    let oldest = null;
                    for (const [reqId, handler] of agentBridge.pending.entries()) {
                        if (!oldest || handler.ts < oldest.ts) {
                            oldest = { reqId, handler, ts: handler.ts };
                        }
                    }
                    if (oldest) {
                        const msg = event.message
                            || event.data?.message
                            || (typeof event.data === 'string' ? event.data : null)
                            || 'agent bridge error';
                        oldest.handler.reject(new Error(msg));
                        agentBridge.pending.delete(oldest.reqId);
                        continue;
                    }
                }

                // Forward PluginOAuthComplete as a dedicated event to renderer (Plan 18-03)
                if (event.type === 'plugin-oauth-complete') {
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
                    logToFile('info', 'daemon whatsapp-link-status event', {
                        state: event.data?.status ?? null,
                        hasLastError: Boolean(event.data?.last_error),
                    });
                    let oldest = null;
                    for (const [reqId, handler] of agentBridge.pending.entries()) {
                        if (pendingHandlerMatchesResponseType(handler, event.type)) {
                            if (!oldest || handler.ts < oldest.ts) {
                                oldest = { reqId, handler, ts: handler.ts };
                            }
                        }
                    }
                    if (oldest) {
                        oldest.handler.resolve(event.data ?? event);
                        agentBridge.pending.delete(oldest.reqId);
                    }
                    if (mainWindow && !mainWindow.isDestroyed()) {
                        const statusPayload = event.data ?? {};
                        if (typeof statusPayload.last_error === 'string' && statusPayload.last_error.trim()) {
                            mainWindow.webContents.send('whatsapp-error', statusPayload.last_error);
                        }
                    }
                    continue;
                }

                if (event.type === 'whatsapp-link-qr') {
                    logToFile('info', 'daemon whatsapp-link-qr event', {
                        asciiLen: typeof event.data?.ascii_qr === 'string' ? event.data.ascii_qr.length : 0,
                        expiresAtMs: event.data?.expires_at_ms ?? null,
                    });
                    if (mainWindow && !mainWindow.isDestroyed()) {
                        convertWhatsAppQrToDataUrl(event.data?.ascii_qr)
                            .then((dataUrl) => {
                                mainWindow?.webContents?.send('whatsapp-qr', dataUrl);
                            })
                            .catch((error) => {
                                mainWindow?.webContents?.send('whatsapp-error', `Failed to render WhatsApp QR: ${error.message || String(error)}`);
                            });
                    }
                    continue;
                }

                if (event.type === 'whatsapp-link-linked') {
                    logToFile('info', 'daemon whatsapp-link-linked event', {
                        phone: event.data?.phone ?? null,
                    });
                    if (mainWindow && !mainWindow.isDestroyed()) {
                        mainWindow.webContents.send('whatsapp-connected', {
                            phone: event.data?.phone || null,
                        });
                    }
                    continue;
                }

                if (event.type === 'whatsapp-link-error') {
                    logToFile('warn', 'daemon whatsapp-link-error event', {
                        message: event.data?.message || null,
                        recoverable: event.data?.recoverable ?? null,
                    });
                    if (mainWindow && !mainWindow.isDestroyed()) {
                        mainWindow.webContents.send('whatsapp-error', event.data?.message || 'WhatsApp link error');
                    }
                    continue;
                }

                if (event.type === 'whatsapp-link-disconnected') {
                    logToFile('info', 'daemon whatsapp-link-disconnected event', {
                        reason: event.data?.reason || null,
                    });
                    if (mainWindow && !mainWindow.isDestroyed()) {
                        const reason = event.data?.reason;
                        if (typeof reason === 'string' && reason.trim()) {
                            mainWindow.webContents.send('whatsapp-error', reason);
                        }
                        mainWindow.webContents.send('whatsapp-disconnected', {
                            reason: typeof reason === 'string' ? reason : null,
                        });
                    }
                    continue;
                }

                // Agent events (delta, done, tool_call, etc.) — forward to renderer
                if (event.type === 'concierge_welcome') {
                    logToFile('info', '[concierge] forwarding concierge_welcome to renderer', { contentLen: event.content?.length, actionsLen: event.actions?.length });
                }
                if (mainWindow && !mainWindow.isDestroyed()) {
                    mainWindow.webContents.send('agent-event', event);
                } else {
                    if (event.type === 'concierge_welcome') {
                        logToFile('warn', '[concierge] mainWindow not available to forward event');
                    }
                }
            }
        });

        bridgeProcess.stderr.on('data', (chunk) => {
            logToFile('warn', 'agent bridge stderr', { message: chunk.toString('utf8').trim() });
        });

        bridgeProcess.on('exit', (code) => {
            logToFile('info', 'agent bridge exited', { code });
            // Reject pending requests
            for (const [, handler] of (agentBridge?.pending ?? new Map()).entries()) {
                handler.reject(new Error('agent bridge exited'));
            }
            agentBridge = null;
            whatsappDaemonSubscribed = false;
            agentBridgeConsecutiveExitCount += 1;
            const cappedBackoffMs = Math.min(5000, 250 * (2 ** Math.min(agentBridgeConsecutiveExitCount, 5)));
            agentBridgeRestartCooldownUntil = Date.now() + cappedBackoffMs;
            logToFile('warn', 'agent bridge restart cooldown', {
                consecutiveExits: agentBridgeConsecutiveExitCount,
                cooldownMs: cappedBackoffMs,
            });
        });

        return agentBridge;
    }

    function ensureDbBridge() {
        if (dbBridge && !dbBridge.process.killed) return dbBridge;

        const cliPath = getDaemonPath().replace(/tamux-daemon/, 'tamux').replace(/tamux-daemon\.exe/, 'tamux.exe');
        if (!fs.existsSync(cliPath)) {
            logToFile('warn', 'db bridge: tamux CLI not found', { cliPath });
            return null;
        }

        const bridgeProcess = spawn(cliPath, ['db-bridge'], {
            cwd: path.dirname(cliPath),
            windowsHide: true,
            stdio: ['pipe', 'pipe', 'pipe'],
        });

        dbBridge = {
            process: bridgeProcess,
            ready: false,
            pending: new Map(),
            stdoutBuffer: '',
        };

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
                    // Reject the oldest pending request with the error message
                    let oldest = null;
                    for (const [reqId, handler] of dbBridge.pending.entries()) {
                        if (!oldest || handler.ts < oldest.ts) {
                            oldest = { reqId, handler, ts: handler.ts };
                        }
                    }
                    if (oldest) {
                        oldest.handler.resolve(null);
                        dbBridge.pending.delete(oldest.reqId);
                    }
                    logToFile('warn', 'db bridge error', { message: event.message || event.data });
                    continue;
                }

                let oldest = null;
                for (const [reqId, handler] of dbBridge.pending.entries()) {
                    if (handler.responseType === event.type) {
                        if (!oldest || handler.ts < oldest.ts) {
                            oldest = { reqId, handler, ts: handler.ts };
                        }
                    }
                }

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

    // Expose to module scope for lifecycle hooks (e.g. WhatsApp unsubscribe on quit)
    sendAgentCommandFn = sendAgentCommand;

    function sendAgentQuery(command, responseType, timeoutMs = 5000) {
        return new Promise((resolve, reject) => {
            const bridge = ensureAgentBridge();
            if (!bridge) {
                reject(new Error('Agent bridge not available'));
                return;
            }
            const responseKey = Array.isArray(responseType)
                ? responseType.join('|')
                : responseType;
            const reqId = `${responseKey}_${Date.now()}`;
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
            if (!bridge) {
                reject(new Error('DB bridge not available'));
                return;
            }
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

    function sendDbAckCommand(command, timeoutMs = 5000) {
        return sendDbQuery(command, 'ack', timeoutMs);
    }

    ipcMain.handle('agent-send-message', async (_event, threadId, content, sessionId, contextMessages) => {
        try {
            logToFile('info', 'agent-send-message', {
                threadId,
                contentLen: content?.length,
                sessionId,
                contextCount: Array.isArray(contextMessages) ? contextMessages.length : 0,
            });
            const cmd = {
                type: 'send-message',
                thread_id: threadId || null,
                content,
                session_id: typeof sessionId === 'string' && sessionId.trim() ? sessionId.trim() : null,
            };
            if (Array.isArray(contextMessages) && contextMessages.length > 0) {
                cmd.context_messages = contextMessages;
                logToFile('info', 'agent-send-message context roles', {
                    roles: contextMessages.map(m => m.role),
                });
            }
            sendAgentCommand(cmd);
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });

    ipcMain.handle('agent-stop-stream', async (_event, threadId) => {
        try {
            sendAgentCommand({ type: 'stop-stream', thread_id: threadId });
            return { ok: true };
        } catch {
            return { ok: true };
        }
    });

    ipcMain.handle('agent-list-threads', async () => {
        try {
            return await sendAgentQuery({ type: 'list-threads' }, 'thread-list');
        } catch {
            return [];
        }
    });

    ipcMain.handle('agent-get-thread', async (_event, threadId) => {
        try {
            return await sendAgentQuery({ type: 'get-thread', thread_id: threadId }, 'thread-detail');
        } catch {
            return null;
        }
    });

    ipcMain.handle('agent-delete-thread', async (_event, threadId) => {
        try {
            sendAgentCommand({ type: 'delete-thread', thread_id: threadId });
            return true;
        } catch {
            return false;
        }
    });

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
                dependencies: Array.isArray(payload?.dependencies)
                    ? payload.dependencies
                        .filter((value) => typeof value === 'string' && value.trim())
                        .map((value) => value.trim())
                    : [],
            });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });

    ipcMain.handle('agent-cancel-task', async (_event, taskId) => {
        try {
            sendAgentCommand({ type: 'cancel-task', task_id: taskId });
            return true;
        } catch {
            return false;
        }
    });

    ipcMain.handle('agent-list-tasks', async () => {
        try {
            return await sendAgentQuery({ type: 'list-tasks' }, 'task-list');
        } catch {
            return [];
        }
    });

    ipcMain.handle('agent-list-runs', async () => {
        try {
            return await sendAgentQuery({ type: 'list-runs' }, 'run-list');
        } catch {
            return [];
        }
    });

    ipcMain.handle('agent-get-run', async (_event, runId) => {
        try {
            return await sendAgentQuery({ type: 'get-run', run_id: runId }, 'run-detail');
        } catch {
            return null;
        }
    });

    ipcMain.handle('agent-list-todos', async () => {
        try {
            return await sendAgentQuery({ type: 'list-todos' }, 'todo-list');
        } catch {
            return {};
        }
    });

    ipcMain.handle('agent-get-todos', async (_event, threadId) => {
        try {
            return await sendAgentQuery({ type: 'get-todos', thread_id: threadId }, 'todo-detail');
        } catch {
            return { thread_id: threadId, items: [] };
        }
    });

    ipcMain.handle('agent-get-work-context', async (_event, threadId) => {
        try {
            return await sendAgentQuery({ type: 'get-work-context', thread_id: threadId }, 'work-context-detail');
        } catch {
            return { thread_id: threadId, context: { thread_id: threadId, entries: [] } };
        }
    });

    ipcMain.handle('agent-get-git-diff', async (_event, repoPath, filePath) => {
        try {
            return await sendAgentQuery({
                type: 'get-git-diff',
                repo_path: repoPath,
                file_path: typeof filePath === 'string' && filePath.trim() ? filePath.trim() : null,
            }, 'git-diff');
        } catch {
            return { repo_path: repoPath, file_path: filePath ?? null, diff: '' };
        }
    });

    ipcMain.handle('agent-get-file-preview', async (_event, filePath, maxBytes) => {
        try {
            return await sendAgentQuery({
                type: 'get-file-preview',
                path: filePath,
                max_bytes: Number.isFinite(maxBytes) ? Math.max(1024, Math.trunc(maxBytes)) : null,
            }, 'file-preview');
        } catch {
            return { path: filePath, content: '', truncated: false, is_text: false };
        }
    });

    ipcMain.handle('agent-start-goal-run', async (_event, payload) => {
        try {
            return await sendAgentQuery({
                type: 'start-goal-run',
                goal: payload?.goal,
                title: typeof payload?.title === 'string' && payload.title.trim() ? payload.title.trim() : null,
                thread_id: typeof payload?.threadId === 'string' && payload.threadId.trim() ? payload.threadId.trim() : null,
                session_id: typeof payload?.sessionId === 'string' && payload.sessionId.trim() ? payload.sessionId.trim() : null,
                priority: typeof payload?.priority === 'string' && payload.priority.trim() ? payload.priority.trim() : null,
                client_request_id: typeof payload?.clientRequestId === 'string' && payload.clientRequestId.trim() ? payload.clientRequestId.trim() : null,
            }, 'goal-run-started');
        } catch (err) {
            return { ok: false, error: err?.message || String(err) };
        }
    });

    ipcMain.handle('agent-list-goal-runs', async () => {
        try {
            return await sendAgentQuery({ type: 'list-goal-runs' }, 'goal-run-list');
        } catch {
            return [];
        }
    });

    ipcMain.handle('agent-get-goal-run', async (_event, goalRunId) => {
        try {
            return await sendAgentQuery({ type: 'get-goal-run', goal_run_id: goalRunId }, 'goal-run-detail');
        } catch {
            return null;
        }
    });

    ipcMain.handle('agent-control-goal-run', async (_event, goalRunId, action, stepIndex) => {
        try {
            return await sendAgentQuery({
                type: 'control-goal-run',
                goal_run_id: goalRunId,
                action,
                step_index: Number.isFinite(stepIndex) ? Math.trunc(stepIndex) : null,
            }, 'goal-run-controlled');
        } catch {
            return { ok: false };
        }
    });

    ipcMain.handle('agent-explain-action', async (_event, actionId, stepIndex) => {
        try {
            return await sendAgentQuery({
                type: 'explain-action',
                action_id: actionId,
                step_index: Number.isFinite(stepIndex) ? Math.trunc(stepIndex) : null,
            }, 'agent-explanation');
        } catch (err) {
            return { ok: false, error: err?.message || String(err) };
        }
    });

    ipcMain.handle('agent-start-divergent-session', async (_event, payload) => {
        try {
            return await sendAgentQuery({
                type: 'start-divergent-session',
                problem_statement: payload?.problemStatement,
                thread_id: payload?.threadId,
                goal_run_id: typeof payload?.goalRunId === 'string' && payload.goalRunId.trim() ? payload.goalRunId.trim() : null,
                custom_framings_json: typeof payload?.customFramingsJson === 'string' && payload.customFramingsJson.trim()
                    ? payload.customFramingsJson
                    : null,
            }, 'agent-divergent-session-started');
        } catch (err) {
            return { ok: false, error: err?.message || String(err) };
        }
    });

    ipcMain.handle('agent-get-divergent-session', async (_event, sessionId) => {
        try {
            return await sendAgentQuery({
                type: 'get-divergent-session',
                session_id: sessionId,
            }, 'agent-divergent-session');
        } catch (err) {
            return { ok: false, error: err?.message || String(err) };
        }
    });

    ipcMain.handle('agent-start-operator-profile-session', async (_event, kind) => {
        try {
            return await sendAgentQuery(
                { type: 'start-operator-profile-session', kind: kind || 'first_run_onboarding' },
                'operator-profile-session-started'
            );
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });

    ipcMain.handle('agent-next-operator-profile-question', async (_event, sessionId) => {
        try {
            return await sendAgentQuery(
                { type: 'next-operator-profile-question', session_id: sessionId },
                ['operator-profile-question', 'operator-profile-session-completed']
            );
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });

    ipcMain.handle('agent-submit-operator-profile-answer', async (_event, sessionId, questionId, answerJson) => {
        try {
            return await sendAgentQuery(
                {
                    type: 'submit-operator-profile-answer',
                    session_id: sessionId,
                    question_id: questionId,
                    answer_json: answerJson,
                },
                ['operator-profile-progress', 'operator-profile-session-completed']
            );
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });

    ipcMain.handle('agent-skip-operator-profile-question', async (_event, sessionId, questionId, reason) => {
        try {
            return await sendAgentQuery(
                {
                    type: 'skip-operator-profile-question',
                    session_id: sessionId,
                    question_id: questionId,
                    reason: typeof reason === 'string' ? reason : null,
                },
                ['operator-profile-progress', 'operator-profile-session-completed']
            );
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });

    ipcMain.handle('agent-defer-operator-profile-question', async (_event, sessionId, questionId, deferUntilUnixMs) => {
        try {
            return await sendAgentQuery(
                {
                    type: 'defer-operator-profile-question',
                    session_id: sessionId,
                    question_id: questionId,
                    defer_until_unix_ms: Number.isFinite(deferUntilUnixMs) ? Math.trunc(deferUntilUnixMs) : null,
                },
                ['operator-profile-progress', 'operator-profile-session-completed']
            );
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });

    ipcMain.handle('agent-get-operator-profile-summary', async () => {
        try {
            return await sendAgentQuery(
                { type: 'get-operator-profile-summary' },
                'operator-profile-summary'
            );
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });

    ipcMain.handle('agent-set-operator-profile-consent', async (_event, consentKey, granted) => {
        try {
            await sendAgentQuery(
                {
                    type: 'set-operator-profile-consent',
                    consent_key: consentKey,
                    granted: Boolean(granted),
                },
                'operator-profile-session-completed'
            );
            return { ok: true };
        } catch (err) {
            return { error: err?.message || String(err) };
        }
    });

    ipcMain.handle('agent-get-concierge-config', async () => {
        try {
            return await sendAgentQuery({ type: 'get-concierge-config' }, 'concierge-config');
        } catch (err) {
            throw err;
        }
    });

    ipcMain.handle('agent-set-concierge-config', async (_event, config) => {
        try {
            sendAgentCommand({ type: 'set-concierge-config', config_json: JSON.stringify(config) });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });

    ipcMain.handle('agent-dismiss-concierge-welcome', async () => {
        try {
            sendAgentCommand({ type: 'dismiss-concierge-welcome' });
            return { ok: true };
        } catch {
            return { ok: false };
        }
    });

    ipcMain.handle('agent-request-concierge-welcome', async () => {
        try {
            sendAgentCommand({ type: 'request-concierge-welcome' });
            return { ok: true };
        } catch {
            return { ok: false };
        }
    });

    ipcMain.handle('dismiss-audit-entry', async (_event, entryId) => {
        try {
            sendAgentCommand({ type: 'audit-dismiss', entry_id: entryId });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });

    ipcMain.handle('agent-get-config', async () => {
        try {
            return await sendAgentQuery({ type: 'get-config' }, 'config');
        } catch (err) {
            throw err;
        }
    });

    ipcMain.handle('agent-get-status', async () => {
        try {
            return await sendAgentQuery({ type: 'get-status' }, 'status-response');
        } catch (err) {
            logToFile('warn', 'agent-get-status failed', { error: err?.message ?? String(err) });
            return null;
        }
    });

    ipcMain.handle('agent-set-config-item', async (_event, keyPath, value) => {
        try {
            sendAgentCommand({
                type: 'set-config-item',
                key_path: keyPath,
                value_json: JSON.stringify(value),
            });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });

    ipcMain.handle('agent-set-provider-model', async (_event, providerId, model) => {
        try {
            sendAgentCommand({
                type: 'set-provider-model',
                provider_id: providerId,
                model,
            });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });

    ipcMain.handle('agent-set-tier-override', async (_event, tier) => {
        try {
            sendAgentCommand({
                type: 'set-tier-override',
                tier: tier || null,
            });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });

    ipcMain.handle('gateway:get-config', async () => {
        try {
            const config = await sendAgentQuery({ type: 'get-config' }, 'config');
            return config?.gateway ?? {};
        } catch (err) {
            logToFile('warn', '[gateway] get-config IPC error', { error: err.message });
            return {};
        }
    });

    ipcMain.handle('gateway:set-config', async (_event, patch) => {
        try {
            for (const [key, value] of Object.entries(patch || {})) {
                sendAgentCommand({
                    type: 'set-config-item',
                    key_path: `gateway.${key}`,
                    value_json: JSON.stringify(value),
                });
            }
            logToFile('info', '[gateway] Config updated via IPC', { keys: Object.keys(patch || {}) });
            return { ok: true };
        } catch (err) {
            logToFile('warn', '[gateway] set-config IPC error', { error: err.message });
            return { ok: false, error: err.message };
        }
    });

    ipcMain.handle('agent-get-provider-auth-states', async () => {
        try {
            return await sendAgentQuery({ type: 'get-provider-auth-states' }, 'provider-auth-states');
        } catch (err) {
            throw err;
        }
    });

    ipcMain.handle('agent-login-provider', async (_event, providerId, apiKey, baseUrl) => {
        try {
            return await sendAgentQuery(
                { type: 'login-provider', provider_id: providerId, api_key: apiKey, base_url: baseUrl || '' },
                'provider-auth-states'
            );
        } catch (err) {
            return { error: err.message };
        }
    });

    ipcMain.handle('agent-logout-provider', async (_event, providerId) => {
        try {
            return await sendAgentQuery(
                { type: 'logout-provider', provider_id: providerId },
                'provider-auth-states'
            );
        } catch (err) {
            return { error: err.message };
        }
    });

    ipcMain.handle('agent-validate-provider', async (_event, providerId, baseUrl, apiKey, authSource) => {
        try {
            return await sendAgentQuery(
                { type: 'validate-provider', provider_id: providerId, base_url: baseUrl, api_key: apiKey, auth_source: authSource },
                'provider-validation'
            );
        } catch (err) {
            return { valid: false, error: err.message };
        }
    });

    ipcMain.handle('agent-set-sub-agent', async (_event, subAgentJson) => {
        try {
            sendAgentCommand({ type: 'set-sub-agent', sub_agent_json: subAgentJson });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });

    ipcMain.handle('agent-remove-sub-agent', async (_event, subAgentId) => {
        try {
            sendAgentCommand({ type: 'remove-sub-agent', sub_agent_id: subAgentId });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });

    ipcMain.handle('agent-list-sub-agents', async () => {
        try {
            return await sendAgentQuery({ type: 'list-sub-agents' }, 'sub-agent-list');
        } catch {
            return [];
        }
    });

    ipcMain.handle('openai-codex-auth-status', async (_event, options) => {
        return openAICodexAuthHandlers.status(_event, options);
    });

    ipcMain.handle('openai-codex-auth-login', async () => {
        return openAICodexAuthHandlers.login();
    });

    ipcMain.handle('openai-codex-auth-logout', async () => {
        return openAICodexAuthHandlers.logout();
    });

    ipcMain.handle('agent-heartbeat-get-items', async () => {
        try {
            return await sendAgentQuery({ type: 'heartbeat-get-items' }, 'heartbeat-items');
        } catch {
            return [];
        }
    });

    ipcMain.handle('agent-heartbeat-set-items', async (_event, items) => {
        try {
            sendAgentCommand({ type: 'heartbeat-set-items', items_json: JSON.stringify(items) });
            return { ok: true };
        } catch (err) {
            return { ok: false, error: err.message };
        }
    });

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

configureChromiumRuntimePaths({ app, logToFile });

app.whenReady().then(async () => {
    logToFile('info', 'electron app ready');

    // Allow cross-origin API calls from the renderer (LLM providers, etc.)
    // Desktop apps don't need browser CORS restrictions.
    session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
        callback({
            responseHeaders: {
                ...details.responseHeaders,
                'access-control-allow-origin': ['*'],
                'access-control-allow-headers': ['*'],
                'access-control-allow-methods': ['*'],
            },
        });
    });

    registerIpcHandlers();
    await spawnDaemon();
    createWindow();

    app.on('activate', () => {
        if (BrowserWindow.getAllWindows().length === 0) createWindow();
    });
});

app.on('before-quit', () => {
    logToFile('info', 'electron before-quit');
    terminalBridgeRuntime.stopAllTerminalBridges(true, true);
    if (whatsappDaemonSubscribed && sendAgentCommandFn) {
        try {
            sendAgentCommandFn({ type: 'whats-app-link-unsubscribe' });
        } catch {
            // Best effort during shutdown.
        }
        whatsappDaemonSubscribed = false;
        whatsappDaemonSubscriptionDesired = false;
    }
    stopWhatsAppBridge();
    cleanupDiscordClient();
});

app.on('will-quit', () => {
    logToFile('info', 'electron will-quit');
    terminalBridgeRuntime.stopAllTerminalBridges(true, true);
    cleanupDiscordClient();
});

app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') {
        terminalBridgeRuntime.stopAllTerminalBridges(true, true);
        app.quit();
        app.exit(0);
    }
});

process.on('exit', () => terminalBridgeRuntime.stopAllTerminalBridges(true, true));
