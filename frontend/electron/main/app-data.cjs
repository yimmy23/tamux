const fs = require('fs');
const os = require('os');
const path = require('path');

function getLegacyAmuxDataDir() {
    if (process.platform === 'win32' && process.env.LOCALAPPDATA) {
        return path.join(process.env.LOCALAPPDATA, 'amux');
    }
    return path.join(os.homedir(), '.amux');
}

function getTamuxDataDir() {
    if (process.platform === 'win32' && process.env.LOCALAPPDATA) {
        return path.join(process.env.LOCALAPPDATA, 'tamux');
    }
    return path.join(os.homedir(), '.tamux');
}

function ensureTamuxDataDir() {
    const dataDir = getTamuxDataDir();
    const legacyDir = getLegacyAmuxDataDir();
    if (!fs.existsSync(dataDir) && fs.existsSync(legacyDir)) {
        try {
            fs.mkdirSync(path.dirname(dataDir), { recursive: true });
            fs.renameSync(legacyDir, dataDir);
        } catch {
            // Ignore migration failure and continue with the new directory.
        }
    }
    fs.mkdirSync(dataDir, { recursive: true });
    return dataDir;
}

function installBundledGuidelines(options = {}) {
    const targetRoot = options.targetRoot || ensureTamuxDataDir();
    const sourceCandidates = Array.isArray(options.sourceCandidates) ? options.sourceCandidates : [];
    const source = sourceCandidates.find((candidate) => {
        try {
            return candidate && fs.existsSync(candidate) && fs.statSync(candidate).isDirectory();
        } catch {
            return false;
        }
    });

    if (!source) {
        return { copied: 0, skipped: 0, source: null };
    }

    const targetDir = path.join(targetRoot, 'guidelines');
    fs.mkdirSync(targetDir, { recursive: true });

    let copied = 0;
    let skipped = 0;
    const copyMissing = (dir) => {
        for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
            const sourcePath = path.join(dir, entry.name);
            const relativePath = path.relative(source, sourcePath);
            const targetPath = path.join(targetDir, relativePath);

            if (entry.isDirectory()) {
                copyMissing(sourcePath);
                continue;
            }

            if (!entry.isFile()) continue;
            if (fs.existsSync(targetPath)) {
                skipped += 1;
                continue;
            }

            fs.mkdirSync(path.dirname(targetPath), { recursive: true });
            fs.copyFileSync(sourcePath, targetPath);
            copied += 1;
        }
    };

    copyMissing(source);
    return { copied, skipped, source };
}

function getVisionTempDir() {
    const dir = path.join(ensureTamuxDataDir(), 'tmp', 'vision');
    fs.mkdirSync(dir, { recursive: true });
    return dir;
}

function getAudioTempDir() {
    const dir = path.join(ensureTamuxDataDir(), 'tmp', 'audio');
    fs.mkdirSync(dir, { recursive: true });
    return dir;
}

function cleanupVisionScreenshots(ttlMs) {
    try {
        const dir = getVisionTempDir();
        const now = Date.now();
        const entries = fs.readdirSync(dir);
        for (const entry of entries) {
            const fullPath = path.join(dir, entry);
            try {
                const stats = fs.statSync(fullPath);
                if (!stats.isFile()) continue;
                if (now - stats.mtimeMs > ttlMs) {
                    fs.unlinkSync(fullPath);
                }
            } catch {
                // Ignore per-file cleanup errors.
            }
        }
    } catch {
        // Ignore cleanup errors.
    }
}

function saveVisionScreenshot(payload = {}, options = {}) {
    const ttlMs = Number.isFinite(options.ttlMs) ? Number(options.ttlMs) : 10 * 60 * 1000;
    try {
        const dataUrl = typeof payload.dataUrl === 'string' ? payload.dataUrl.trim() : '';
        if (!dataUrl.startsWith('data:image/png;base64,')) {
            return { ok: false, error: 'Invalid PNG data URL' };
        }

        cleanupVisionScreenshots(ttlMs);

        const base64 = dataUrl.slice('data:image/png;base64,'.length);
        const buffer = Buffer.from(base64, 'base64');
        const now = Date.now();
        const filename = `ss_${now}_${Math.random().toString(36).slice(2, 8)}.png`;
        const fullPath = path.join(getVisionTempDir(), filename);
        fs.writeFileSync(fullPath, buffer);

        setTimeout(() => {
            try {
                if (fs.existsSync(fullPath)) {
                    fs.unlinkSync(fullPath);
                }
            } catch {
                // Ignore deferred cleanup errors.
            }
        }, ttlMs);

        return {
            ok: true,
            path: fullPath,
            expiresAt: now + ttlMs,
        };
    } catch (error) {
        return { ok: false, error: error?.message ?? String(error) };
    }
}

function saveTempAudioCapture(payload = {}, options = {}) {
    const ttlMs = Number.isFinite(options.ttlMs) ? Number(options.ttlMs) : 10 * 60 * 1000;
    try {
        const base64 = typeof payload.base64 === 'string' ? payload.base64.trim() : '';
        if (!base64) {
            return { ok: false, error: 'Missing audio base64 payload' };
        }

        const mimeType = typeof payload.mimeType === 'string' && payload.mimeType.trim()
            ? payload.mimeType.trim()
            : 'audio/webm';
        const extension = mimeType === 'audio/wav'
            ? 'wav'
            : mimeType === 'audio/ogg'
                ? 'ogg'
                : mimeType === 'audio/flac'
                    ? 'flac'
                    : mimeType === 'audio/mp4'
                        ? 'm4a'
                        : mimeType === 'audio/mpeg'
                            ? 'mp3'
                            : mimeType === 'audio/webm'
                                ? 'webm'
                                : 'bin';
        const buffer = Buffer.from(base64, 'base64');
        const now = Date.now();
        const filename = `audio_${now}_${Math.random().toString(36).slice(2, 8)}.${extension}`;
        const fullPath = path.join(getAudioTempDir(), filename);
        fs.writeFileSync(fullPath, buffer);

        setTimeout(() => {
            try {
                if (fs.existsSync(fullPath)) {
                    fs.unlinkSync(fullPath);
                }
            } catch {
                // Ignore deferred cleanup errors.
            }
        }, ttlMs);

        return {
            ok: true,
            path: fullPath,
            mimeType,
            expiresAt: now + ttlMs,
        };
    } catch (error) {
        return { ok: false, error: error?.message ?? String(error) };
    }
}

function configureChromiumRuntimePaths(options = {}) {
    const { app, logToFile } = options;

    try {
        const dataDir = ensureTamuxDataDir();
        const userDataDir = path.join(dataDir, 'electron-profile');
        const cacheDir = path.join(dataDir, 'chromium-cache');

        fs.mkdirSync(userDataDir, { recursive: true });
        fs.mkdirSync(cacheDir, { recursive: true });

        app.setPath('userData', userDataDir);
        app.setPath('sessionData', cacheDir);
        app.commandLine.appendSwitch('disk-cache-dir', cacheDir);
    } catch (error) {
        logToFile?.('warn', 'failed to configure chromium runtime paths', {
            message: error?.message ?? String(error),
        });
    }

    const settingsPath = path.join(getTamuxDataDir(), 'settings.json');
    let gpuEnabled = true;
    try {
        const raw = fs.readFileSync(settingsPath, 'utf-8');
        const parsed = JSON.parse(raw);
        if ((parsed.settings?.gpuAcceleration ?? parsed.gpuAcceleration) === false) {
            gpuEnabled = false;
        }
    } catch {}

    if (!gpuEnabled) {
        app.disableHardwareAcceleration();
        app.commandLine.appendSwitch('disable-gpu');
        app.commandLine.appendSwitch('disable-gpu-compositing');
        app.commandLine.appendSwitch('disable-gpu-shader-disk-cache');
        app.commandLine.appendSwitch('disable-gpu-program-cache');
    }
}

function resolveDataPath(relativePath) {
    if (typeof relativePath !== 'string' || !relativePath.trim()) {
        throw new Error('A relative path is required.');
    }

    const baseDir = path.resolve(ensureTamuxDataDir());
    const normalized = path.normalize(relativePath).replace(/^(\.\.(\\|\/|$))+/, '');
    const targetPath = path.resolve(baseDir, normalized);

    if (targetPath !== baseDir && !targetPath.startsWith(`${baseDir}${path.sep}`)) {
        throw new Error('Path escapes the tamux data directory.');
    }

    return targetPath;
}

function readJsonFile(relativePath) {
    const filePath = resolveDataPath(relativePath);
    if (!fs.existsSync(filePath)) {
        return null;
    }

    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

async function writeJsonFile(relativePath, data) {
    const filePath = resolveDataPath(relativePath);
    await fs.promises.mkdir(path.dirname(filePath), { recursive: true });
    await fs.promises.writeFile(filePath, JSON.stringify(data, null, 2), 'utf8');
    return true;
}

function readTextFile(relativePath) {
    const filePath = resolveDataPath(relativePath);
    if (!fs.existsSync(filePath)) {
        return null;
    }

    return fs.readFileSync(filePath, 'utf8');
}

async function writeTextFile(relativePath, content) {
    const filePath = resolveDataPath(relativePath);
    await fs.promises.mkdir(path.dirname(filePath), { recursive: true });
    await fs.promises.writeFile(filePath, typeof content === 'string' ? content : '', 'utf8');
    return true;
}

function deleteDataPath(relativePath) {
    const filePath = resolveDataPath(relativePath);
    if (!fs.existsSync(filePath)) {
        return false;
    }

    fs.rmSync(filePath, { recursive: true, force: true });
    return true;
}

function listDataDir(relativeDir = '') {
    const dirPath = resolveDataPath(relativeDir || '.');
    if (!fs.existsSync(dirPath) || !fs.statSync(dirPath).isDirectory()) {
        return [];
    }

    return fs.readdirSync(dirPath, { withFileTypes: true }).map((entry) => {
        const absolutePath = path.join(dirPath, entry.name);
        return {
            name: entry.name,
            path: path.relative(ensureTamuxDataDir(), absolutePath).replace(/\\/g, '/'),
            isDirectory: entry.isDirectory(),
        };
    });
}

function openDataPath(relativePath, shell) {
    const filePath = resolveDataPath(relativePath);
    if (!fs.existsSync(filePath)) {
        return 'Path does not exist';
    }

    return shell.openPath(filePath);
}

function revealDataPath(relativePath, shell) {
    const filePath = resolveDataPath(relativePath);
    if (!fs.existsSync(filePath)) {
        return false;
    }

    shell.showItemInFolder(filePath);
    return true;
}

function datedLogFileName(fileName, date = new Date()) {
    const stem = fileName.endsWith('.log') ? fileName.slice(0, -4) : fileName;
    const isoDate = date.toISOString().slice(0, 10);
    return `${stem}-${isoDate}.log`;
}

function logToFile(level, message, details) {
    try {
        const logDir = getTamuxDataDir();
        fs.mkdirSync(logDir, { recursive: true });
        const now = new Date();
        const line = [
            now.toISOString(),
            level.toUpperCase(),
            message,
            details ? JSON.stringify(details) : '',
        ].filter(Boolean).join(' ') + '\n';
        fs.appendFileSync(path.join(logDir, datedLogFileName('tamux-electron.log', now)), line, 'utf8');
    } catch {
        // Ignore logging failures.
    }
}

module.exports = {
    cleanupVisionScreenshots,
    configureChromiumRuntimePaths,
    datedLogFileName,
    deleteDataPath,
    ensureTamuxDataDir,
    installBundledGuidelines,
    getLegacyAmuxDataDir,
    getTamuxDataDir,
    getVisionTempDir,
    getAudioTempDir,
    listDataDir,
    logToFile,
    openDataPath,
    readJsonFile,
    readTextFile,
    resolveDataPath,
    revealDataPath,
    saveTempAudioCapture,
    saveVisionScreenshot,
    writeJsonFile,
    writeTextFile,
};
