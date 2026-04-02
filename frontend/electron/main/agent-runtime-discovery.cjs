const fs = require('fs');
const net = require('net');
const os = require('os');
const path = require('path');
const { spawnSync } = require('child_process');

const KNOWN_CODING_AGENTS = [
    {
        id: 'claude',
        label: 'Claude Code',
        description: "Anthropic's terminal coding agent.",
        executables: ['claude'],
        versionArgs: ['--version'],
        launchArgs: [],
    },
    {
        id: 'codex',
        label: 'Codex CLI',
        description: 'OpenAI Codex terminal workflow.',
        executables: ['codex'],
        versionArgs: ['--version'],
        launchArgs: [],
    },
    {
        id: 'gemini',
        label: 'Gemini CLI',
        description: 'Google Gemini terminal agent.',
        executables: ['gemini'],
        versionArgs: ['--version'],
        launchArgs: [],
    },
    {
        id: 'hermes',
        label: 'Hermes Agent',
        description: 'Nous Research Hermes agent runtime.',
        executables: ['hermes'],
        versionArgs: ['--version'],
        configPaths: ['~/.hermes/config.yaml', '~/.hermes/.env'],
        launchArgs: [],
    },
    {
        id: 'pi',
        label: 'pi.dev',
        description: 'Pi terminal coding harness.',
        executables: ['pi'],
        versionArgs: ['--version'],
        configPaths: ['~/.pi/agent/settings.json', '~/.pi/agent/sessions', '~/.pi/agent/AGENTS.md'],
        launchArgs: [],
    },
    {
        id: 'opencode',
        label: 'OpenCode',
        description: 'OpenCode terminal coding assistant.',
        executables: ['opencode'],
        versionArgs: ['--version'],
        launchArgs: [],
    },
    {
        id: 'openclaw',
        label: 'OpenClaw',
        description: 'OpenClaw agent and gateway runtime.',
        executables: ['openclaw'],
        versionArgs: ['--version'],
        configPaths: ['~/.openclaw/openclaw.json', '~/.openclaw/workspace', '~/.openclaw/state'],
        launchArgs: [],
    },
    {
        id: 'kimi',
        label: 'Kimi CLI',
        description: 'Moonshot Kimi coding assistant.',
        executables: ['kimi'],
        versionArgs: ['--version'],
        launchArgs: [],
    },
    {
        id: 'aider',
        label: 'Aider',
        description: 'Aider pair-programming CLI.',
        executables: ['aider'],
        versionArgs: ['--version'],
        launchArgs: [],
    },
    {
        id: 'goose',
        label: 'Goose',
        description: 'Goose local coding agent.',
        executables: ['goose'],
        versionArgs: ['--version'],
        launchArgs: [],
    },
    {
        id: 'qwen-coder',
        label: 'Qwen Coder',
        description: 'Qwen coding CLI if installed locally.',
        executables: ['qwen', 'qwen-coder'],
        versionArgs: ['--version'],
        launchArgs: [],
    },
];

const KNOWN_AI_TRAINING = [
    {
        id: 'prime-verifiers',
        label: 'Prime Intellect Verifiers',
        kind: 'training-runtime',
        description: 'Prime Intellect environments, evaluation, and RL workflow runtime.',
        executables: ['prime'],
        versionArgs: ['--version'],
        systemChecks: [
            { label: 'prime CLI', path: 'prime', type: 'command' },
            { label: 'uv', path: 'uv', type: 'command' },
        ],
        workspaceChecks: [
            { label: 'configs/', path: 'configs' },
            { label: 'environments/', path: 'environments' },
            { label: 'AGENTS.md', path: 'AGENTS.md' },
        ],
    },
    {
        id: 'autoresearch',
        label: 'AutoResearch',
        kind: 'repository-workflow',
        description: 'Karpathy\'s repo-local autonomous research loop for a single-GPU training harness.',
        executables: ['uv'],
        versionArgs: ['--version'],
        systemChecks: [
            { label: 'uv', path: 'uv', type: 'command' },
            { label: 'python3', path: 'python3', type: 'command' },
            { label: 'git', path: 'git', type: 'command' },
        ],
        workspaceChecks: [
            { label: 'program.md', path: 'program.md' },
            { label: 'train.py', path: 'train.py' },
            { label: 'prepare.py', path: 'prepare.py' },
            { label: 'pyproject.toml', path: 'pyproject.toml' },
        ],
    },
    {
        id: 'autorl',
        label: 'AutoRL',
        kind: 'repository-workflow',
        description: 'Repo-local autonomous RL environment search scaffold backed by Simverse.',
        executables: ['python3'],
        versionArgs: ['--version'],
        systemChecks: [
            { label: 'python3', path: 'python3', type: 'command' },
            { label: 'git', path: 'git', type: 'command' },
        ],
        workspaceChecks: [
            { label: 'program.md', path: 'program.md' },
            { label: 'train.py', path: 'train.py' },
            { label: 'framework.py', path: 'framework.py' },
            { label: 'vendor/simverse', path: 'vendor/simverse' },
            { label: '.venv', path: '.venv' },
        ],
    },
];

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
function resolveExecutablePath(binary) {
    if (typeof binary !== 'string' || !binary.trim()) return null;
    try {
        const checker = process.platform === 'win32' ? 'where' : 'which';
        const result = spawnSync(checker, [binary], {
            encoding: 'utf8',
            timeout: 5000,
            windowsHide: true,
        });
        if (result.status !== 0) {
            return null;
        }

        const firstLine = `${result.stdout || ''}`.split(/\r?\n/).map((entry) => entry.trim()).find(Boolean);
        return firstLine || null;
    } catch {
        return null;
    }
}
function probeExecutableVersion(commandPath, versionArgs = ['--version']) {
    if (!commandPath) {
        return null;
    }

    try {
        const result = spawnSync(commandPath, versionArgs, {
            encoding: 'utf8',
            timeout: 5000,
            windowsHide: true,
        });

        const output = `${result.stdout || result.stderr || ''}`.split(/\r?\n/).map((entry) => entry.trim()).find(Boolean);
        return output || null;
    } catch {
        return null;
    }
}
function expandHomePath(targetPath) {
    if (typeof targetPath !== 'string' || !targetPath.trim()) {
        return targetPath;
    }

    if (targetPath === '~') {
        return os.homedir();
    }

    if (targetPath.startsWith('~/')) {
        return path.join(os.homedir(), targetPath.slice(2));
    }

    return targetPath;
}
function collectConfigChecks(paths = []) {
    return paths.map((entry) => {
        const expandedPath = expandHomePath(entry);
        const resolvedPath = path.resolve(expandedPath);
        return {
            label: path.basename(entry) || entry,
            path: entry,
            exists: fs.existsSync(resolvedPath),
        };
    });
}
function resolveWorkspacePath(workspacePath) {
    if (typeof workspacePath !== 'string' || !workspacePath.trim()) {
        return null;
    }

    return path.resolve(expandHomePath(workspacePath));
}
function collectAITrainingChecks(definition, workspacePath) {
    const checks = [];

    for (const check of definition.systemChecks || []) {
        const exists = check.type === 'command'
            ? commandExists(check.path)
            : fs.existsSync(path.resolve(expandHomePath(check.path)));
        checks.push({
            label: check.label,
            path: check.path,
            exists,
            scope: 'system',
        });
    }

    for (const check of definition.workspaceChecks || []) {
        const targetPath = workspacePath ? path.join(workspacePath, check.path) : null;
        checks.push({
            label: check.label,
            path: check.path,
            exists: targetPath ? fs.existsSync(targetPath) : false,
            scope: 'workspace',
        });
    }

    return checks;
}
function hasWorkspaceChecks(checks, paths) {
    return paths.every((targetPath) => checks.some((check) => check.scope === 'workspace' && check.path === targetPath && check.exists));
}
function summarizeRuntimeReadiness(agent, available, checks, gatewayReachable) {
    if (!available) {
        return {
            readiness: 'missing',
            runtimeNotes: [`${agent.label} is not installed on PATH.`],
        };
    }

    const existingChecks = checks.filter((check) => check.exists);
    const missingChecks = checks.filter((check) => !check.exists);
    const runtimeNotes = [];

    if (agent.id === 'hermes') {
        if (existingChecks.length > 0) {
            runtimeNotes.push('Hermes configuration was detected. Consider wiring tamux-mcp into Hermes MCP settings for deeper integration.');
            return { readiness: 'ready', runtimeNotes };
        }

        runtimeNotes.push('Hermes is installed, but no ~/.hermes config was detected yet. Run hermes setup before expecting provider-backed workflows.');
        return { readiness: 'needs-setup', runtimeNotes };
    }

    if (agent.id === 'openclaw') {
        if (existingChecks.length === 0) {
            runtimeNotes.push('OpenClaw is installed, but no ~/.openclaw runtime files were detected yet. Run onboarding before expecting gateway-backed workflows.');
            return { readiness: 'needs-setup', runtimeNotes };
        }

        if (gatewayReachable === true) {
            runtimeNotes.push('OpenClaw gateway responded on 127.0.0.1:18789.');
            return { readiness: 'ready', runtimeNotes };
        }

        runtimeNotes.push('OpenClaw configuration is present, but the local gateway did not respond on 127.0.0.1:18789. Direct agent mode should still be usable.');
        return { readiness: 'needs-setup', runtimeNotes };
    }

    if (agent.id === 'pi') {
        if (existingChecks.length > 0) {
            runtimeNotes.push('Pi configuration and session storage were detected under ~/.pi/agent.');
            return { readiness: 'ready', runtimeNotes };
        }

        runtimeNotes.push('pi is installed, but no ~/.pi/agent profile was detected yet. Run pi once and complete provider login or API-key setup.');
        return { readiness: 'needs-setup', runtimeNotes };
    }

    if (missingChecks.length > 0 && existingChecks.length === 0 && checks.length > 0) {
        runtimeNotes.push(`${agent.label} is installed, but none of the known config paths were detected.`);
        return { readiness: 'needs-setup', runtimeNotes };
    }

    return { readiness: 'ready', runtimeNotes };
}
function checkLocalTcpPort(host, port, timeoutMs = 300) {
    return new Promise((resolve) => {
        const socket = net.createConnection({ host, port });
        let settled = false;

        const finish = (value) => {
            if (settled) {
                return;
            }
            settled = true;
            try {
                socket.destroy();
            } catch {
                // Ignore socket cleanup errors.
            }
            resolve(value);
        };

        socket.setTimeout(timeoutMs);
        socket.once('connect', () => finish(true));
        socket.once('timeout', () => finish(false));
        socket.once('error', () => finish(false));
        socket.once('close', () => finish(false));
    });
}
async function discoverCodingAgents() {
    const discovered = await Promise.all(KNOWN_CODING_AGENTS.map(async (agent) => {
        const executable = agent.executables.find((candidate) => resolveExecutablePath(candidate)) || null;
        const commandPath = executable ? resolveExecutablePath(executable) : null;
        const checks = collectConfigChecks(agent.configPaths || []);
        const gatewayReachable = agent.id === 'openclaw'
            ? await checkLocalTcpPort('127.0.0.1', 18789)
            : null;
        const readinessSummary = summarizeRuntimeReadiness(agent, Boolean(commandPath), checks, gatewayReachable);

        return {
            id: agent.id,
            available: Boolean(commandPath),
            executable,
            path: commandPath,
            version: commandPath ? probeExecutableVersion(commandPath, agent.versionArgs) : null,
            readiness: readinessSummary.readiness,
            checks,
            runtimeNotes: readinessSummary.runtimeNotes,
            gatewayLabel: agent.id === 'openclaw' ? '127.0.0.1:18789' : null,
            gatewayReachable,
            error: commandPath ? null : `${agent.label} was not found on PATH.`,
        };
    }));

    return discovered;
}
function summarizeAITrainingReadiness(definition, available, checks, workspacePath) {
    if (!available) {
        return {
            readiness: 'missing',
            runtimeNotes: [`${definition.label} is missing a required system dependency.`],
        };
    }

    const runtimeNotes = [];
    const systemChecks = checks.filter((check) => check.scope === 'system');
    const workspaceChecks = checks.filter((check) => check.scope === 'workspace');
    const missingSystem = systemChecks.filter((check) => !check.exists);
    const presentWorkspace = workspaceChecks.filter((check) => check.exists);
    const missingWorkspace = workspaceChecks.filter((check) => !check.exists);

    if (missingSystem.length > 0) {
        runtimeNotes.push(`Missing system prerequisites: ${missingSystem.map((check) => check.label).join(', ')}.`);
        return { readiness: 'missing', runtimeNotes };
    }

    if (!workspacePath) {
        runtimeNotes.push('Select a workspace with a configured cwd to evaluate repository-specific training readiness.');
        return { readiness: 'needs-setup', runtimeNotes };
    }

    if (definition.id === 'prime-verifiers') {
        if (missingWorkspace.length === 0) {
            runtimeNotes.push('Prime lab workspace files were detected and should be ready for evaluation or environment work.');
            return { readiness: 'ready', runtimeNotes };
        }

        runtimeNotes.push('Prime CLI is available, but this workspace does not look fully initialized. Run prime lab setup in the target workspace.');
        return { readiness: 'needs-setup', runtimeNotes };
    }

    if (definition.id === 'autoresearch') {
        if (missingWorkspace.length === 0) {
            runtimeNotes.push('AutoResearch repo files were detected. A compatible GPU is still required for meaningful training runs.');
            return { readiness: 'ready', runtimeNotes };
        }

        runtimeNotes.push('This workspace is missing one or more AutoResearch files. Clone the repo and keep program.md, train.py, prepare.py, and pyproject.toml together.');
        return { readiness: 'needs-setup', runtimeNotes };
    }

    if (definition.id === 'autorl') {
        const baseReady = presentWorkspace.some((check) => check.path === 'program.md')
            && presentWorkspace.some((check) => check.path === 'train.py')
            && presentWorkspace.some((check) => check.path === 'framework.py')
            && presentWorkspace.some((check) => check.path === 'vendor/simverse');
        const venvReady = presentWorkspace.some((check) => check.path === '.venv');

        if (baseReady && venvReady) {
            runtimeNotes.push('AutoRL workspace and virtual environment were detected. The evaluator should be runnable from this workspace.');
            return { readiness: 'ready', runtimeNotes };
        }

        if (baseReady) {
            runtimeNotes.push('AutoRL repo files were detected, but .venv is missing. Create the virtualenv and install vendor/simverse before evaluator runs.');
            return { readiness: 'needs-setup', runtimeNotes };
        }

        runtimeNotes.push('This workspace does not look like the AutoRL scaffold yet. Clone the repo branch and keep vendor/simverse plus the training files together.');
        return { readiness: 'needs-setup', runtimeNotes };
    }

    return { readiness: 'ready', runtimeNotes };
}
async function discoverAITraining(workspacePath) {
    const resolvedWorkspacePath = resolveWorkspacePath(workspacePath);

    return Promise.all(KNOWN_AI_TRAINING.map(async (definition) => {
        const systemExecutable = definition.executables.find((candidate) => resolveExecutablePath(candidate)) || null;
        const systemPath = systemExecutable ? resolveExecutablePath(systemExecutable) : null;
        const checks = collectAITrainingChecks(definition, resolvedWorkspacePath);
        const readinessSummary = summarizeAITrainingReadiness(definition, Boolean(systemPath), checks, resolvedWorkspacePath);
        let available = Boolean(systemPath);
        let executable = systemExecutable;
        let runtimePath = systemPath;
        let error = systemPath ? null : `${definition.label} prerequisites were not found on PATH.`;

        if (definition.kind === 'repository-workflow') {
            const baseWorkspaceReady = definition.id === 'autoresearch'
                ? hasWorkspaceChecks(checks, ['program.md', 'train.py', 'prepare.py', 'pyproject.toml'])
                : hasWorkspaceChecks(checks, ['program.md', 'train.py', 'framework.py', 'vendor/simverse']);

            available = Boolean(systemPath) && baseWorkspaceReady;
            executable = definition.id === 'autoresearch'
                ? 'uv run train.py'
                : '.venv/bin/python train.py';
            runtimePath = resolvedWorkspacePath;

            if (!resolvedWorkspacePath) {
                error = 'Select a workspace with a configured cwd.';
            } else if (!systemPath) {
                error = `${definition.label} is missing one or more required system tools.`;
            } else if (!baseWorkspaceReady) {
                error = `${definition.label} repository files were not detected in the selected workspace.`;
            } else {
                error = null;
            }
        }

        return {
            id: definition.id,
            available,
            executable,
            path: runtimePath,
            version: systemPath ? probeExecutableVersion(systemPath, definition.versionArgs) : null,
            readiness: readinessSummary.readiness,
            checks,
            runtimeNotes: readinessSummary.runtimeNotes,
            workspacePath: resolvedWorkspacePath,
            error,
        };
    }));
}

module.exports = {
    discoverAITraining,
    discoverCodingAgents,
    resolveExecutablePath,
};
