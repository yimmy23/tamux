const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFile } = require('child_process');
const { promisify } = require('util');

const execFileAsync = promisify(execFile);

function resolveFsPath(targetPath) {
    if (typeof targetPath !== 'string' || !targetPath.trim()) {
        throw new Error('A path is required.');
    }

    const expanded = targetPath.startsWith('~/')
        ? path.join(os.homedir(), targetPath.slice(2))
        : targetPath;
    return path.resolve(expanded);
}

function listFsDir(targetDir) {
    const resolvedDir = resolveFsPath(targetDir || '.');
    if (!fs.existsSync(resolvedDir) || !fs.statSync(resolvedDir).isDirectory()) {
        return [];
    }

    return fs.readdirSync(resolvedDir, { withFileTypes: true }).map((entry) => {
        const absolutePath = path.join(resolvedDir, entry.name);
        let stats = null;
        try {
            stats = fs.statSync(absolutePath);
        } catch {
            stats = null;
        }
        return {
            name: entry.name,
            path: absolutePath,
            isDirectory: entry.isDirectory(),
            sizeBytes: stats?.size ?? null,
            modifiedAt: stats?.mtimeMs ?? null,
        };
    });
}

function copyFsPath(sourcePath, destinationPath) {
    const source = resolveFsPath(sourcePath);
    const destination = resolveFsPath(destinationPath);
    const sourceStats = fs.statSync(source);

    if (sourceStats.isDirectory()) {
        fs.cpSync(source, destination, { recursive: true, force: true });
    } else {
        fs.mkdirSync(path.dirname(destination), { recursive: true });
        fs.copyFileSync(source, destination);
    }
    return true;
}

function moveFsPath(sourcePath, destinationPath) {
    const source = resolveFsPath(sourcePath);
    const destination = resolveFsPath(destinationPath);
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    fs.renameSync(source, destination);
    return true;
}

function deleteFsPath(targetPath) {
    const resolved = resolveFsPath(targetPath);
    if (!fs.existsSync(resolved)) return false;
    fs.rmSync(resolved, { recursive: true, force: true });
    return true;
}

function createFsDirectory(targetDirPath) {
    const resolved = resolveFsPath(targetDirPath);
    fs.mkdirSync(resolved, { recursive: true });
    return true;
}

function getFsPathInfo(targetPath) {
    const resolved = resolveFsPath(targetPath);
    if (!fs.existsSync(resolved)) {
        return null;
    }

    const stats = fs.statSync(resolved);
    return {
        path: resolved,
        isDirectory: stats.isDirectory(),
        sizeBytes: stats.size,
        modifiedAt: stats.mtimeMs,
        createdAt: stats.birthtimeMs,
    };
}

function readFsText(targetPath) {
    const resolved = resolveFsPath(targetPath);
    if (!fs.existsSync(resolved) || fs.statSync(resolved).isDirectory()) {
        return null;
    }
    return fs.readFileSync(resolved, 'utf8');
}

async function writeFsText(targetPath, content) {
    const resolved = resolveFsPath(targetPath);
    await fs.promises.mkdir(path.dirname(resolved), { recursive: true });
    await fs.promises.writeFile(resolved, typeof content === 'string' ? content : '', 'utf8');
    return true;
}

async function resolveGitRepoRoot(targetPath) {
    const resolved = resolveFsPath(targetPath || '.');

    try {
        const { stdout } = await execFileAsync('git', ['rev-parse', '--show-toplevel'], {
            cwd: resolved,
            encoding: 'utf8',
            timeout: 5000,
        });
        const repoRoot = stdout.trim();
        return repoRoot ? resolveFsPath(repoRoot) : null;
    } catch {
        return null;
    }
}

async function gitStatus(targetPath) {
    const repoRoot = await resolveGitRepoRoot(targetPath);
    if (!repoRoot) {
        return '';
    }

    const { stdout } = await execFileAsync('git', ['status', '--short', '--untracked-files=all'], {
        cwd: repoRoot,
        encoding: 'utf8',
        timeout: 5000,
        maxBuffer: 1024 * 1024,
    });
    return stdout;
}

async function gitDiff(targetPath, filePath) {
    const repoRoot = await resolveGitRepoRoot(targetPath);
    if (!repoRoot) {
        return '';
    }

    const relativePath = typeof filePath === 'string' && filePath.trim() ? filePath.trim() : null;
    if (!relativePath) {
        const { stdout } = await execFileAsync('git', ['diff', '--no-ext-diff', 'HEAD'], {
            cwd: repoRoot,
            encoding: 'utf8',
            timeout: 5000,
            maxBuffer: 1024 * 1024 * 4,
        }).catch((error) => {
            if (typeof error?.stdout === 'string') {
                return { stdout: error.stdout };
            }
            return { stdout: '' };
        });
        return stdout;
    }

    const absoluteFilePath = path.resolve(repoRoot, relativePath);
    const headExists = await execFileAsync('git', ['rev-parse', '--verify', 'HEAD'], {
        cwd: repoRoot,
        encoding: 'utf8',
        timeout: 5000,
    }).then(() => true).catch(() => false);
    const tracked = await execFileAsync('git', ['ls-files', '--error-unmatch', '--', relativePath], {
        cwd: repoRoot,
        encoding: 'utf8',
        timeout: 5000,
    }).then(() => true).catch(() => false);

    if (!tracked && fs.existsSync(absoluteFilePath)) {
        const untrackedDiff = await execFileAsync(
            'git',
            ['diff', '--no-index', '--no-ext-diff', '--', '/dev/null', absoluteFilePath],
            {
                cwd: repoRoot,
                encoding: 'utf8',
                timeout: 5000,
                maxBuffer: 1024 * 1024 * 4,
            },
        ).catch((error) => {
            if (typeof error?.stdout === 'string') {
                return { stdout: error.stdout };
            }
            return { stdout: '' };
        });
        return untrackedDiff.stdout;
    }

    const args = headExists
        ? ['diff', '--no-ext-diff', 'HEAD', '--', relativePath]
        : ['diff', '--no-ext-diff', '--cached', '--', relativePath];
    const { stdout } = await execFileAsync('git', args, {
        cwd: repoRoot,
        encoding: 'utf8',
        timeout: 5000,
        maxBuffer: 1024 * 1024 * 4,
    }).catch((error) => {
        if (typeof error?.stdout === 'string') {
            return { stdout: error.stdout };
        }
        return { stdout: '' };
    });
    return stdout;
}

module.exports = {
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
};
