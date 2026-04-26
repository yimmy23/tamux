"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const install = require("./install");

test("getReleaseAssetInfo maps linux x64 to published zip asset names", function () {
  const info = install.getReleaseAssetInfo("linux", "x64", "0.2.0");

  assert.deepEqual(info, {
    archiveName: "tamux-linux-x86_64.zip",
    checksumName: "SHA256SUMS-linux-x86_64.txt",
    bundleChecksumName: "SHA256SUMS.txt",
    skillsArchiveRoot: "skills",
    guidelinesArchiveRoot: "guidelines",
    requiredBinaries: [
      "tamux",
      "tamux-daemon",
      "tamux-tui",
      "tamux-gateway",
      "tamux-mcp",
    ],
  });
});

test("getReleaseAssetInfo maps linux arm64 to published zip asset names", function () {
  const info = install.getReleaseAssetInfo("linux", "arm64", "0.2.0");

  assert.deepEqual(info, {
    archiveName: "tamux-linux-aarch64.zip",
    checksumName: "SHA256SUMS-linux-aarch64.txt",
    bundleChecksumName: "SHA256SUMS.txt",
    skillsArchiveRoot: "skills",
    guidelinesArchiveRoot: "guidelines",
    requiredBinaries: [
      "tamux",
      "tamux-daemon",
      "tamux-tui",
      "tamux-gateway",
      "tamux-mcp",
    ],
  });
});

test("getReleaseAssetInfo maps windows x64 to published zip asset names", function () {
  const info = install.getReleaseAssetInfo("win32", "x64", "0.2.0");

  assert.deepEqual(info, {
    archiveName: "tamux-windows-x64.zip",
    checksumName: "SHA256SUMS-windows-x64.txt",
    bundleChecksumName: "SHA256SUMS.txt",
    skillsArchiveRoot: "skills",
    guidelinesArchiveRoot: "guidelines",
    requiredBinaries: [
      "tamux.exe",
      "tamux-daemon.exe",
      "tamux-tui.exe",
      "tamux-gateway.exe",
      "tamux-mcp.exe",
    ],
  });
});

test("getReleaseAssetInfo returns null for unsupported targets", function () {
  assert.equal(install.getReleaseAssetInfo("linux", "ppc64"), null);
});

test("getInstallUsageHint recommends npx for local installs", function () {
  assert.equal(
    install.getInstallUsageHint(false),
    "tamux: run with 'npx tamux --help' (or 'npm exec tamux -- --help') after a local install"
  );
});

test("getInstallUsageHint recommends tamux for global installs", function () {
  assert.equal(
    install.getInstallUsageHint(true),
    "tamux: run 'tamux --help' once your npm global bin directory is on PATH, and open a new shell if it is not recognized immediately"
  );
});

test("getGlobalBinDir appends bin on unix platforms", function () {
  assert.equal(
    install.getGlobalBinDir("/opt/homebrew", "darwin"),
    "/opt/homebrew/bin"
  );
  assert.equal(install.getGlobalBinDir("/usr/local", "linux"), "/usr/local/bin");
});

test("getInstallUsageHint includes explicit global bin directory when known", function () {
  assert.equal(
    install.getInstallUsageHint(true, "/opt/homebrew/bin"),
    "tamux: if 'tamux' is not found, add '/opt/homebrew/bin' to PATH, then open a new shell and run 'tamux --help'"
  );
});

test("prependDirectoryToPath prefixes PATH values", function () {
  const updated = install.prependDirectoryToPath(
    { PATH: "/usr/bin" },
    "/tmp/tamux-bin"
  );

  assert.equal(updated.PATH, "/tmp/tamux-bin:/usr/bin");
});

test("prependDirectoryToPath preserves existing PATH key casing", function () {
  const updated = install.prependDirectoryToPath(
    { Path: "C:\\Windows\\System32" },
    "C:\\tamux\\bin"
  );

  const expected = ["C:\\tamux\\bin", "C:\\Windows\\System32"].join(path.delimiter);
  assert.equal(updated.Path, expected);
  assert.equal(updated.PATH, expected);
});

test("getRuntimeTamuxRoot uses home on unix hosts", function () {
  assert.equal(
    install.getRuntimeTamuxRoot("linux", {
      HOME: "/home/aline",
    }),
    "/home/aline/.tamux"
  );
});

test("getRuntimeTamuxRoot uses LOCALAPPDATA on windows hosts", function () {
  assert.equal(
    install.getRuntimeTamuxRoot("win32", {
      LOCALAPPDATA: "C:\\Users\\aline\\AppData\\Local",
    }),
    "C:\\Users\\aline\\AppData\\Local\\tamux"
  );
});

test("getRuntimeSkillsDir resolves to the canonical tamux skills root", function () {
  assert.equal(
    install.getRuntimeSkillsDir("linux", {
      HOME: "/home/aline",
    }),
    "/home/aline/.tamux/skills"
  );
});

test("getRuntimeGuidelinesDir resolves beside the canonical tamux skills root", function () {
  assert.equal(
    install.getRuntimeGuidelinesDir("linux", {
      HOME: "/home/aline",
    }),
    "/home/aline/.tamux/guidelines"
  );

  assert.equal(
    install.getRuntimeGuidelinesDir("win32", {
      LOCALAPPDATA: "C:\\Users\\aline\\AppData\\Local",
    }),
    "C:\\Users\\aline\\AppData\\Local\\tamux\\guidelines"
  );
});

test("getRuntimeCustomAuthPath resolves beside daemon runtime data", function () {
  assert.equal(
    install.getRuntimeCustomAuthPath("linux", {
      HOME: "/home/aline",
    }),
    "/home/aline/.tamux/custom-auth.yaml"
  );

  assert.equal(
    install.getRuntimeCustomAuthPath("win32", {
      LOCALAPPDATA: "C:\\Users\\aline\\AppData\\Local",
    }),
    "C:\\Users\\aline\\AppData\\Local\\tamux\\custom-auth.yaml"
  );
});

test("ensureCustomAuthTemplate creates default yaml without overwriting", function () {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "tamux-custom-auth-"));
  const customAuthPath = install.getRuntimeCustomAuthPath("linux", { HOME: root });

  install.ensureCustomAuthTemplate("linux", { HOME: root });

  assert.equal(
    fs.readFileSync(customAuthPath, "utf8"),
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

  fs.writeFileSync(customAuthPath, "providers:\n  - id: already-here\n");
  install.ensureCustomAuthTemplate("linux", { HOME: root });

  assert.equal(
    fs.readFileSync(customAuthPath, "utf8"),
    "providers:\n  - id: already-here\n"
  );
});

test("release bundle metadata includes bundled skills payload", function () {
  const info = install.getReleaseAssetInfo("linux", "x64", "0.2.0");

  assert.equal(info.skillsArchiveRoot, "skills");
  assert.equal(info.guidelinesArchiveRoot, "guidelines");
});

test("extractBundledGuidelines copies missing defaults without overwriting user files", function () {
  const AdmZip = require("adm-zip");
  const zip = new AdmZip();
  zip.addFile("guidelines/coding-task.md", Buffer.from("# bundled coding\n"));
  zip.addFile("guidelines/research-task.md", Buffer.from("# bundled research\n"));

  const root = fs.mkdtempSync(path.join(os.tmpdir(), "tamux-guidelines-"));
  const guidelinesDir = path.join(root, "guidelines");
  fs.mkdirSync(guidelinesDir, { recursive: true });
  fs.writeFileSync(path.join(guidelinesDir, "coding-task.md"), "# user coding\n");

  install.extractBundledGuidelines(
    zip.toBuffer(),
    { guidelinesArchiveRoot: "guidelines" },
    guidelinesDir
  );

  assert.equal(
    fs.readFileSync(path.join(guidelinesDir, "coding-task.md"), "utf8"),
    "# user coding\n"
  );
  assert.equal(
    fs.readFileSync(path.join(guidelinesDir, "research-task.md"), "utf8"),
    "# bundled research\n"
  );
});

test("global npm install stops processes before replacing binaries and restarts daemon after success", async function () {
  const events = [];

  await install.maybeRefreshDaemonAfterInstall(
    {
      isGlobalInstall: true,
      platform: "linux",
      binDir: "/tmp/tamux-bin",
    },
    async function () {
      events.push("install");
    },
    {
      stopProcesses: async function (platform) {
        events.push("stop:" + platform);
      },
      startDaemon: function (platform, binDir) {
        events.push("start:" + platform + ":" + binDir);
        return path.join(binDir, "tamux-daemon");
      },
    }
  );

  assert.deepEqual(events, [
    "stop:linux",
    "install",
    "start:linux:/tmp/tamux-bin",
  ]);
});

test("local npm install does not stop or restart daemon", async function () {
  const events = [];

  await install.maybeRefreshDaemonAfterInstall(
    {
      isGlobalInstall: false,
      platform: "linux",
      binDir: "/tmp/tamux-bin",
    },
    async function () {
      events.push("install");
    },
    {
      stopProcesses: async function () {
        events.push("stop");
      },
      startDaemon: function () {
        events.push("start");
      },
    }
  );

  assert.deepEqual(events, ["install"]);
});
