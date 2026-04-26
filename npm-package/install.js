#!/usr/bin/env node
// install.js -- download platform-specific tamux binaries on npm install
"use strict";

const https = require("https");
const http = require("http");
const fs = require("fs");
const path = require("path");
const os = require("os");
const crypto = require("crypto");
const childProcess = require("child_process");

const VERSION = require("./package.json").version;
const BIN_DIR = path.join(__dirname, "bin");

// GitHub owner/repo for release asset downloads.
const GITHUB_OWNER = "mkurman";
const GITHUB_REPO = "tamux";

const PLATFORM_MAP = {
  "linux-x64": {
    archivePlatform: "linux-x86_64",
    checksumPlatform: "linux-x86_64",
    requiredBinaries: ["tamux", "tamux-daemon", "tamux-tui", "tamux-gateway", "tamux-mcp"],
  },
  "linux-arm64": {
    archivePlatform: "linux-aarch64",
    checksumPlatform: "linux-aarch64",
    requiredBinaries: ["tamux", "tamux-daemon", "tamux-tui", "tamux-gateway", "tamux-mcp"],
  },
  "darwin-arm64": {
    archivePlatform: "darwin-arm64",
    checksumPlatform: "darwin-arm64",
    requiredBinaries: ["tamux", "tamux-daemon", "tamux-tui", "tamux-gateway", "tamux-mcp"],
  },
  "darwin-x64": {
    archivePlatform: "darwin-x86_64",
    checksumPlatform: "darwin-x86_64",
    requiredBinaries: ["tamux", "tamux-daemon", "tamux-tui", "tamux-gateway", "tamux-mcp"],
  },
  "win32-x64": {
    archivePlatform: "windows-x64",
    checksumPlatform: "windows-x64",
    requiredBinaries: ["tamux.exe", "tamux-daemon.exe", "tamux-tui.exe", "tamux-gateway.exe", "tamux-mcp.exe"],
  },
};

const BASE_URL = `https://github.com/${GITHUB_OWNER}/${GITHUB_REPO}/releases/download/v${VERSION}`;
const PROCESS_STOP_TIMEOUT_MS = 5000;
const PROCESS_POLL_INTERVAL_MS = 100;

/**
 * Download a URL to a Buffer, following up to maxRedirects HTTP 301/302 redirects.
 * @param {string} url
 * @param {number} [maxRedirects=5]
 * @returns {Promise<Buffer>}
 */
function download(url, maxRedirects) {
  if (maxRedirects === undefined) maxRedirects = 5;
  return new Promise(function (resolve, reject) {
    var proto = url.startsWith("https://") ? https : http;
    proto
      .get(url, function (res) {
        // Follow redirects
        if (
          (res.statusCode === 301 || res.statusCode === 302) &&
          res.headers.location
        ) {
          if (maxRedirects <= 0) {
            reject(new Error("Too many redirects"));
            return;
          }
          resolve(download(res.headers.location, maxRedirects - 1));
          return;
        }

        if (res.statusCode !== 200) {
          reject(
            new Error("Download failed: HTTP " + res.statusCode + " for " + url)
          );
          return;
        }

        var chunks = [];
        res.on("data", function (chunk) {
          chunks.push(chunk);
        });
        res.on("end", function () {
          resolve(Buffer.concat(chunks));
        });
        res.on("error", reject);
      })
      .on("error", reject);
  });
}

/**
 * Verify SHA256 checksum of a file against an expected hex digest.
 * @param {string} filePath
 * @param {string} expectedHash
 * @returns {Promise<boolean>}
 */
function verifyChecksum(filePath, expectedHash) {
  return new Promise(function (resolve, reject) {
    var hash = crypto.createHash("sha256");
    var stream = fs.createReadStream(filePath);
    stream.on("data", function (chunk) {
      hash.update(chunk);
    });
    stream.on("end", function () {
      resolve(hash.digest("hex") === expectedHash);
    });
    stream.on("error", reject);
  });
}

/**
 * Parse a SHA256SUMS file to extract the hash for a given filename.
 * Expected format: "<hex_hash>  <filename>" (two spaces, matching sha256sum output).
 * @param {string} content
 * @param {string} filename
 * @returns {string|null}
 */
function parseChecksumFile(content, filename) {
  var lines = content.toString("utf8").trim().split("\n");
  for (var i = 0; i < lines.length; i++) {
    var line = lines[i].trim();
    if (!line) continue;
    // Format: hash  filename  OR  hash *filename
    var parts = line.split(/\s+/);
    if (parts.length >= 2) {
      var hash = parts[0];
      var name = parts[parts.length - 1].replace(/^\*/, "");
      if (name === filename) return hash;
    }
  }
  return null;
}

function getArchiveChecksum(checksumsData, releaseInfo) {
  if (!releaseInfo) {
    return null;
  }

  return parseChecksumFile(checksumsData, releaseInfo.archiveName);
}

function getRuntimeTamuxRoot(platform, env) {
  var sourceEnv = env || process.env;
  if (platform === "win32") {
    var localAppData = sourceEnv.LOCALAPPDATA;
    if (localAppData) {
      return path.win32.join(localAppData, "tamux");
    }

    var userProfile = sourceEnv.USERPROFILE || "";
    return path.win32.join(userProfile, "AppData", "Local", "tamux");
  }

  return path.posix.join(sourceEnv.HOME || "", ".tamux");
}

function getRuntimeSkillsDir(platform, env) {
  var pathModule = platform === "win32" ? path.win32 : path.posix;
  return pathModule.join(getRuntimeTamuxRoot(platform, env), "skills");
}

function getRuntimeGuidelinesDir(platform, env) {
  var pathModule = platform === "win32" ? path.win32 : path.posix;
  return pathModule.join(getRuntimeTamuxRoot(platform, env), "guidelines");
}

function getRuntimeCustomAuthPath(platform, env) {
  var pathModule = platform === "win32" ? path.win32 : path.posix;
  return pathModule.join(getRuntimeTamuxRoot(platform, env), "custom-auth.yaml");
}

function ensureCustomAuthTemplate(platform, env) {
  var customAuthPath = getRuntimeCustomAuthPath(platform, env);
  fs.mkdirSync(path.dirname(customAuthPath), { recursive: true });

  if (fs.existsSync(customAuthPath)) {
    return customAuthPath;
  }

  fs.writeFileSync(
    customAuthPath,
    "# Add named custom providers here. The daemon reloads this file before\n" +
      "# provider/model setup in the TUI and desktop app.\n" +
      "# Prefer api_key_env for secrets, for example:\n" +
      "# providers:\n" +
      "#   - id: local-openai\n" +
      "#     name: Local OpenAI-Compatible\n" +
      "#     default_base_url: http://127.0.0.1:11434/v1\n" +
      "#     default_model: llama3.3\n" +
      "#     api_key_env: LOCAL_OPENAI_API_KEY\n" +
      "providers: []\n"
  );
  return customAuthPath;
}

function verifyBufferChecksum(buffer, expectedHash) {
  return crypto.createHash("sha256").update(buffer).digest("hex") === expectedHash;
}

function getReleaseAssetInfo(platform, arch, version) {
  var key = platform + "-" + arch;
  var target = PLATFORM_MAP[key];
  void version;

  if (!target) {
    return null;
  }

  return {
    archiveName: "tamux-" + target.archivePlatform + ".zip",
    checksumName: "SHA256SUMS-" + target.checksumPlatform + ".txt",
    bundleChecksumName: "SHA256SUMS.txt",
    requiredBinaries: target.requiredBinaries.slice(),
    skillsArchiveRoot: "skills",
    guidelinesArchiveRoot: "guidelines",
  };
}

function getGlobalBinDir(prefix, platform) {
  if (!prefix) {
    return null;
  }

  if (platform === "win32") {
    return prefix;
  }

  return path.join(prefix, "bin");
}

function getInstallUsageHint(isGlobalInstall, globalBinDir) {
  if (isGlobalInstall) {
    if (globalBinDir) {
      return (
        "tamux: if 'tamux' is not found, add '" +
        globalBinDir +
        "' to PATH, then open a new shell and run 'tamux --help'"
      );
    }

    return "tamux: run 'tamux --help' once your npm global bin directory is on PATH, and open a new shell if it is not recognized immediately";
  }

  return "tamux: run with 'npx tamux --help' (or 'npm exec tamux -- --help') after a local install";
}

function prependDirectoryToPath(env, directory) {
  var nextEnv = Object.assign({}, env);
  var pathKey =
    Object.keys(nextEnv).find(function (key) {
      return key.toUpperCase() === "PATH";
    }) || "PATH";
  var currentPath = nextEnv[pathKey] || "";
  var nextPath = currentPath
    ? directory + path.delimiter + currentPath
    : directory;

  nextEnv[pathKey] = nextPath;
  if (pathKey !== "PATH" && nextEnv.PATH === undefined) {
    nextEnv.PATH = nextPath;
  }

  return nextEnv;
}

function getManagedProcessName(target, platform) {
  if (platform === "win32") {
    return target + ".exe";
  }

  return target;
}

function getKillCommand(platform, target) {
  var processName = getManagedProcessName(target, platform);
  if (platform === "win32") {
    return {
      command: "taskkill",
      args: ["/IM", processName, "/F"],
    };
  }

  return {
    command: "pkill",
    args: ["-x", processName],
  };
}

function getProbeCommand(platform, target) {
  var processName = getManagedProcessName(target, platform);
  if (platform === "win32") {
    return {
      command: "tasklist",
      args: ["/FI", "IMAGENAME eq " + processName],
      processName: processName,
    };
  }

  return {
    command: "pgrep",
    args: ["-x", processName],
    processName: processName,
  };
}

function sleep(ms) {
  return new Promise(function (resolve) {
    setTimeout(resolve, ms);
  });
}

function isManagedProcessRunning(platform, target, execFileSyncImpl) {
  var execFileSync = execFileSyncImpl || childProcess.execFileSync;
  var probe = getProbeCommand(platform, target);

  try {
    var stdout = execFileSync(probe.command, probe.args, {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });

    if (platform === "win32") {
      return stdout.toLowerCase().includes(probe.processName.toLowerCase());
    }

    return true;
  } catch (error) {
    if (platform === "win32") {
      var output = String(error.stdout || "");
      return output.toLowerCase().includes(probe.processName.toLowerCase());
    }

    if (error && error.status === 1) {
      return false;
    }

    throw error;
  }
}

async function stopManagedTamuxProcesses(platform, deps) {
  var options = deps || {};
  var execFileSync = options.execFileSync || childProcess.execFileSync;
  var sleepImpl = options.sleep || sleep;
  var targets = ["tamux-gateway", "tamux-daemon"];

  for (var i = 0; i < targets.length; i++) {
    var target = targets[i];
    var kill = getKillCommand(platform, target);

    try {
      execFileSync(kill.command, kill.args, {
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
      });
    } catch (_error) {
      if (!isManagedProcessRunning(platform, target, execFileSync)) {
        continue;
      }
    }

    var deadline = Date.now() + PROCESS_STOP_TIMEOUT_MS;
    while (Date.now() < deadline) {
      if (!isManagedProcessRunning(platform, target, execFileSync)) {
        break;
      }

      await sleepImpl(PROCESS_POLL_INTERVAL_MS);
    }

    if (isManagedProcessRunning(platform, target, execFileSync)) {
      throw new Error("Timed out waiting for " + target + " to exit");
    }
  }
}

function startManagedDaemon(platform, binDir, spawnImpl) {
  var spawn = spawnImpl || childProcess.spawn;
  var daemonPath = path.join(binDir, getManagedProcessName("tamux-daemon", platform));
  var child = spawn(daemonPath, [], {
    detached: true,
    stdio: "ignore",
    windowsHide: true,
  });

  if (typeof child.unref === "function") {
    child.unref();
  }

  return daemonPath;
}

async function maybeRefreshDaemonAfterInstall(options, installWork, deps) {
  var settings = options || {};
  var helpers = deps || {};
  var stopProcesses = helpers.stopProcesses || stopManagedTamuxProcesses;
  var startDaemon = helpers.startDaemon || startManagedDaemon;

  if (!settings.isGlobalInstall) {
    return installWork();
  }

  console.log("tamux: stopping existing daemon before replacing binaries...");
  await stopProcesses(settings.platform);
  var result = await installWork();
  var daemonPath = startDaemon(settings.platform, settings.binDir);
  console.log("tamux: restarted daemon from " + daemonPath);
  return result;
}

function extractRequiredBinaries(archiveData, releaseInfo) {
  var AdmZip = require("adm-zip");
  var archive = new AdmZip(archiveData);
  var entries = archive.getEntries();

  for (var i = 0; i < releaseInfo.requiredBinaries.length; i++) {
    var binaryName = releaseInfo.requiredBinaries[i];
    var entry = entries.find(function (item) {
      return item.entryName === binaryName;
    });

    if (!entry) {
      throw new Error(
        "Release bundle is missing required binary " + binaryName
      );
    }

    fs.writeFileSync(path.join(BIN_DIR, binaryName), entry.getData());
  }
}

function extractBundledTree(archiveData, archiveRootName, targetDir, skipExisting) {
  var AdmZip = require("adm-zip");
  var archive = new AdmZip(archiveData);
  var entries = archive.getEntries();
  var archiveRoot = archiveRootName + "/";

  fs.mkdirSync(targetDir, { recursive: true });

  for (var i = 0; i < entries.length; i++) {
    var entry = entries[i];
    if (!entry.entryName.startsWith(archiveRoot) || entry.isDirectory) {
      continue;
    }

    var relativePath = entry.entryName.slice(archiveRoot.length);
    if (!relativePath) {
      continue;
    }

    var destinationPath = path.join(targetDir, relativePath);
    if (skipExisting && fs.existsSync(destinationPath)) {
      continue;
    }

    fs.mkdirSync(path.dirname(destinationPath), { recursive: true });
    fs.writeFileSync(destinationPath, entry.getData());
  }
}

function extractBundledSkills(archiveData, releaseInfo, skillsDir) {
  extractBundledTree(archiveData, releaseInfo.skillsArchiveRoot, skillsDir, false);
}

function extractBundledGuidelines(archiveData, releaseInfo, guidelinesDir) {
  extractBundledTree(archiveData, releaseInfo.guidelinesArchiveRoot, guidelinesDir, true);
}

async function verifyExtractedBinaries(checksumsData, releaseInfo) {
  for (var i = 0; i < releaseInfo.requiredBinaries.length; i++) {
    var binaryName = releaseInfo.requiredBinaries[i];
    var expectedHash = parseChecksumFile(checksumsData, binaryName);
    if (!expectedHash) {
      throw new Error(
        "Could not find checksum for required binary " + binaryName
      );
    }

    var binaryPath = path.join(BIN_DIR, binaryName);
    if (!fs.existsSync(binaryPath)) {
      throw new Error("Required binary was not extracted: " + binaryName);
    }

    var valid = await verifyChecksum(binaryPath, expectedHash);
    if (!valid) {
      throw new Error("SHA256 checksum mismatch for " + binaryName);
    }
  }
}

function cleanupExtractedBinaries(releaseInfo) {
  if (!releaseInfo) {
    return;
  }

  for (var i = 0; i < releaseInfo.requiredBinaries.length; i++) {
    try {
      fs.unlinkSync(path.join(BIN_DIR, releaseInfo.requiredBinaries[i]));
    } catch (_e) {
      /* ignore cleanup errors */
    }
  }
}

async function main() {
  var releaseInfo = getReleaseAssetInfo(os.platform(), os.arch(), VERSION);
  var platformKey = os.platform() + "-" + os.arch();
  var targetLabel = releaseInfo
    ? releaseInfo.archiveName
        .replace("tamux-", "")
        .replace(/\.zip$/, "")
    : "unsupported";

  // --dry-run: print platform detection info and exit without downloading
  if (process.argv.includes("--dry-run")) {
    console.log("tamux install.js dry-run");
    console.log("  platform key: " + platformKey);
    console.log("  target:       " + targetLabel);
    console.log("  version:      " + VERSION);
    console.log("  bin dir:      " + BIN_DIR);
    if (releaseInfo) {
      console.log("  download URL: " + BASE_URL + "/" + releaseInfo.archiveName);
      console.log("  checksum URL: " + BASE_URL + "/" + releaseInfo.checksumName);
    }
    process.exit(0);
  }

  if (!releaseInfo) {
    console.warn(
      "tamux: unsupported platform " + platformKey + ", skipping binary download"
    );
    process.exit(0);
  }

  var archiveUrl = BASE_URL + "/" + releaseInfo.archiveName;
  var checksumsUrl = BASE_URL + "/" + releaseInfo.checksumName;
  var isGlobalInstall = process.env.npm_config_global === "true";
  var globalBinDir = getGlobalBinDir(process.env.npm_config_prefix, os.platform());
  var runtimeSkillsDir = getRuntimeSkillsDir(os.platform(), process.env);
  var runtimeGuidelinesDir = getRuntimeGuidelinesDir(os.platform(), process.env);

  // 1. Ensure bin directory exists
  fs.mkdirSync(BIN_DIR, { recursive: true });

  // 2. Download SHA256SUMS file
  console.log("tamux: downloading checksums...");
  var checksumsData = await download(checksumsUrl);

  // 3. Download bundle zip
  console.log("tamux: downloading binaries for " + releaseInfo.archiveName + "...");
  try {
    var archiveData = await download(archiveUrl);
    var archiveChecksum = getArchiveChecksum(checksumsData, releaseInfo);

    if (archiveChecksum) {
      console.log("tamux: verifying SHA256 checksum...");
      if (!verifyBufferChecksum(archiveData, archiveChecksum)) {
        throw new Error("SHA256 checksum mismatch for " + releaseInfo.archiveName);
      }
      console.log("tamux: checksum OK");
    }

    await maybeRefreshDaemonAfterInstall(
      {
        isGlobalInstall: isGlobalInstall,
        platform: os.platform(),
        binDir: BIN_DIR,
      },
      async function () {
        console.log("tamux: extracting binaries, skills, and guidelines...");
        extractRequiredBinaries(archiveData, releaseInfo);
        extractBundledSkills(archiveData, releaseInfo, runtimeSkillsDir);
        extractBundledGuidelines(archiveData, releaseInfo, runtimeGuidelinesDir);
        console.log(
          "tamux: custom provider template ready at " +
            ensureCustomAuthTemplate(os.platform(), process.env)
        );

        if (!archiveChecksum) {
          console.log("tamux: verifying SHA256 checksum...");
          await verifyExtractedBinaries(checksumsData, releaseInfo);
          console.log("tamux: checksum OK");
        }

        if (os.platform() !== "win32") {
          for (var i = 0; i < releaseInfo.requiredBinaries.length; i++) {
            var binPath = path.join(BIN_DIR, releaseInfo.requiredBinaries[i]);
            if (fs.existsSync(binPath)) {
              fs.chmodSync(binPath, 0o755);
            }
          }
        }
      }
    );
  } catch (err) {
    cleanupExtractedBinaries(releaseInfo);
    throw err;
  }

  console.log("tamux: installation complete");
  console.log(getInstallUsageHint(isGlobalInstall, globalBinDir));
}

module.exports = main;
module.exports.GITHUB_OWNER = GITHUB_OWNER;
module.exports.GITHUB_REPO = GITHUB_REPO;
module.exports.getGlobalBinDir = getGlobalBinDir;
module.exports.getArchiveChecksum = getArchiveChecksum;
module.exports.getReleaseAssetInfo = getReleaseAssetInfo;
module.exports.getInstallUsageHint = getInstallUsageHint;
module.exports.getKillCommand = getKillCommand;
module.exports.getManagedProcessName = getManagedProcessName;
module.exports.getProbeCommand = getProbeCommand;
module.exports.getRuntimeGuidelinesDir = getRuntimeGuidelinesDir;
module.exports.getRuntimeSkillsDir = getRuntimeSkillsDir;
module.exports.getRuntimeCustomAuthPath = getRuntimeCustomAuthPath;
module.exports.getRuntimeTamuxRoot = getRuntimeTamuxRoot;
module.exports.ensureCustomAuthTemplate = ensureCustomAuthTemplate;
module.exports.extractBundledGuidelines = extractBundledGuidelines;
module.exports.isManagedProcessRunning = isManagedProcessRunning;
module.exports.maybeRefreshDaemonAfterInstall = maybeRefreshDaemonAfterInstall;
module.exports.parseChecksumFile = parseChecksumFile;
module.exports.prependDirectoryToPath = prependDirectoryToPath;
module.exports.startManagedDaemon = startManagedDaemon;
module.exports.stopManagedTamuxProcesses = stopManagedTamuxProcesses;

// Auto-run only when executed directly (postinstall) or via tryFallbackDownload,
// not when required just for the exported constants.
if (require.main === module) {
  main().catch(function (err) {
    console.warn("tamux: postinstall binary download failed: " + err.message);
    console.warn("tamux: binaries will be downloaded on first run");
    process.exit(0);
  });
}
