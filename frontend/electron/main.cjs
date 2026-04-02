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
const { registerAgentIpcHandlers } = require('./main/agent-ipc-handlers.cjs');
const { createAgentDbBridgeRuntime } = require('./main/agent-db-bridge-runtime.cjs');
const { registerCoreIpcHandlers } = require('./main/core-ipc-handlers.cjs');
const { registerDbIpcHandlers } = require('./main/db-ipc-handlers.cjs');
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
const { createWhatsAppRuntime } = require('./main/whatsapp-runtime.cjs');
const { createWindowRuntime } = require('./main/window-runtime.cjs');

const DAEMON_NAME = 'tamux-daemon';
const CLI_NAME = 'tamux';
const DAEMON_TCP_HOST = '127.0.0.1';
const DAEMON_TCP_PORT = 17563;
const CLONE_SESSION_PREFIX = 'clone:';
const MAX_TERMINAL_HISTORY_BYTES = 1024 * 1024;
const MAX_REATTACH_HISTORY_BYTES = 64 * 1024;
const VISION_SCREENSHOT_TTL_MS = 10 * 60 * 1000;
let mainWindow = null;
// Module-level reference to sendAgentCommand (set during registerIpcHandlers)
let sendAgentCommandFn = null;

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
const windowRuntime = createWindowRuntime({
    app,
    BrowserWindow,
    Menu,
    electronDir: __dirname,
    getMainWindow: () => mainWindow,
    logToFile,
    path,
    screen,
    setMainWindow: (nextWindow) => {
        mainWindow = nextWindow;
    },
    shell,
    stopAllTerminalBridges: terminalBridgeRuntime.stopAllTerminalBridges,
});

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

const whatsAppRuntime = createWhatsAppRuntime({
    electronDir: __dirname,
    fs,
    getMainWindow: () => mainWindow,
    logToFile,
    path,
    pathToFileURL,
    processRef: process,
    spawn,
});

const agentDbBridgeRuntime = createAgentDbBridgeRuntime({
    fs,
    getDaemonPath,
    getMainWindow: () => mainWindow,
    logToFile,
    pendingHandlerMatchesResponseType,
    resolvePendingAgentQueryEvent,
    sendRenderedWhatsAppQr: async (asciiQr) => {
        if (!mainWindow || mainWindow.isDestroyed()) return;
        try {
            const dataUrl = await convertWhatsAppQrToDataUrl(asciiQr);
            mainWindow.webContents.send('whatsapp-qr', dataUrl);
        } catch (error) {
            mainWindow.webContents.send('whatsapp-error', `Failed to render WhatsApp QR: ${error.message || String(error)}`);
        }
    },
    setWhatsAppSubscribed: whatsAppRuntime.setDaemonSubscribed,
    shouldRestoreWhatsAppSubscription: whatsAppRuntime.isSubscriptionRestoreNeeded,
    spawn,
});

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
            return true;
        }
    }
    console.warn('[tamux] Daemon did not become ready');
    logToFile('error', 'daemon did not become ready');
    return false;
}

function registerIpcHandlers() {
    const { sendAgentCommand, sendAgentQuery, sendDbAckCommand, sendDbQuery } = agentDbBridgeRuntime;
    const openAICodexAuthHandlers = createOpenAICodexAuthHandlers(sendAgentQuery);

    registerCoreIpcHandlers(ipcMain, {
        aiTrainingDiscover: discoverAITraining,
        app,
        checkDaemonRunning,
        checkLspHealth,
        checkMcpHealth,
        checkSetupPrereqs,
        clipboard,
        codingAgentsDiscover: discoverCodingAgents,
        copyFsPath,
        createFsDirectory,
        deleteDataPath,
        deleteFsPath,
        discordSendMessage: sendDiscordMessage,
        ensureTamuxDataDir,
        getAvailableShells,
        getDaemonPath,
        getFsPathInfo,
        getPlatform: () => process.platform,
        getSocketPath: () => {
            const endpoint = getDaemonEndpoint();
            return endpoint.path ?? `${endpoint.host}:${endpoint.port}`;
        },
        getSystemFonts,
        getSystemMonitorSnapshot,
        gitDiff,
        gitStatus,
        listDataDir,
        listFsDir,
        loadInstalledPlugins: loadInstalledPluginScripts,
        moveFsPath,
        openDataPath,
        openExternalPath: (targetPath) => shell.openPath(resolveFsPath(targetPath)),
        openExternalRelativePath: (relativePath) => openDataPath(relativePath, shell),
        pluginHandlers: {
            disableDaemon: async (_event, name) => {
                try {
                    sendAgentCommand({ type: 'plugin-disable', name });
                    return { ok: true };
                } catch (err) {
                    return { ok: false, error: err.message };
                }
            },
            enableDaemon: async (_event, name) => {
                try {
                    sendAgentCommand({ type: 'plugin-enable', name });
                    return { ok: true };
                } catch (err) {
                    return { ok: false, error: err.message };
                }
            },
            getDaemon: async (_event, name) => {
                try {
                    return await sendAgentQuery({ type: 'plugin-get', name }, 'plugin-get-result');
                } catch {
                    return { plugin: null, settings_schema: null };
                }
            },
            getSettings: async (_event, name) => {
                try {
                    return await sendAgentQuery({ type: 'plugin-get-settings', name }, 'plugin-settings');
                } catch {
                    return { plugin_name: name, settings: [] };
                }
            },
            listDaemon: async () => {
                try {
                    return await sendAgentQuery({ type: 'plugin-list' }, 'plugin-list-result');
                } catch {
                    return { plugins: [] };
                }
            },
            listInstalled: listInstalledPlugins,
            startOAuth: async (_event, name) => {
                try {
                    const result = await sendAgentQuery({ type: 'plugin-oauth-start', name }, 'plugin-oauth-url', 30000);
                    if (result?.url) {
                        shell.openExternal(result.url);
                    }
                    return { name: result?.name ?? name, url: result?.url ?? '' };
                } catch (err) {
                    return { name, url: '', error: err.message || 'Failed to start OAuth flow' };
                }
            },
            testConnection: async (_event, name) => {
                try {
                    return await sendAgentQuery({ type: 'plugin-test-connection', name }, 'plugin-test-connection-result');
                } catch (err) {
                    return { plugin_name: name, success: false, message: err.message || 'Bridge error' };
                }
            },
            updateSettings: async (_event, pluginName, key, value, isSecret) => {
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
            },
        },
        readFsText,
        readJsonFile,
        readTextFile,
        revealDataPath: (relativePath) => revealDataPath(relativePath, shell),
        revealFsPath: (targetPath) => {
            shell.showItemInFolder(resolveFsPath(targetPath));
            return true;
        },
        saveVisionScreenshot: (payload) => saveVisionScreenshot(payload, { ttlMs: VISION_SCREENSHOT_TTL_MS }),
        setWindowOpacity: windowRuntime.setWindowOpacity,
        spawnDaemon,
        terminalBridgeRuntime,
        windowState: () => mainWindow,
        writeFsText,
        writeJsonFile,
        writeTextFile,
    });
    registerDbIpcHandlers(ipcMain, { sendDbAckCommand, sendDbQuery });

    whatsAppRuntime.registerWhatsAppIpcHandlers(ipcMain, { sendAgentCommand, sendAgentQuery }, async (config) => {
        const { assertValidWhatsAppConnectConfig } = await whatsAppRuntime.getDesktopWhatsAppLinkingHelpers();
        assertValidWhatsAppConnectConfig(config);
    });

    registerAgentIpcHandlers(ipcMain, { sendAgentCommand, sendAgentQuery }, { logToFile, openAICodexAuthHandlers });
    sendAgentCommandFn = sendAgentCommand;
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
    windowRuntime.createWindow();

    app.on('activate', () => {
        if (BrowserWindow.getAllWindows().length === 0) windowRuntime.createWindow();
    });
});

app.on('before-quit', () => {
    logToFile('info', 'electron before-quit');
    terminalBridgeRuntime.stopAllTerminalBridges(true, true);
    if (whatsAppRuntime.isDaemonSubscribed() && sendAgentCommandFn) {
        try {
            sendAgentCommandFn({ type: 'whats-app-link-unsubscribe' });
        } catch {
            // Best effort during shutdown.
        }
    }
    whatsAppRuntime.stopWhatsAppRuntime();
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
