"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const childProcess = require("node:child_process");
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const install = require("./install");

function computeSha256Hex(value) {
  return crypto.createHash("sha256").update(value).digest("hex");
}

test("shell installer dry-run targets GitHub release zip assets", { skip: process.platform === "win32" }, function () {
  const releaseInfo = install.getReleaseAssetInfo(os.platform(), os.arch(), "0.4.2");

  assert.ok(releaseInfo, "expected host platform to be supported by release asset mapping");

  const scriptPath = path.join(__dirname, "..", "scripts", "install.sh");
  const output = childProcess.execFileSync("sh", [scriptPath, "--dry-run"], {
    cwd: path.join(__dirname, ".."),
    env: {
      ...process.env,
      TAMUX_VERSION: "0.4.2",
    },
    encoding: "utf8",
  });

  const expectedUrl = `https://github.com/${install.GITHUB_OWNER}/${install.GITHUB_REPO}/releases/download/v0.4.2/${releaseInfo.archiveName}`;

  assert.match(output, new RegExp(expectedUrl.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  assert.doesNotMatch(output, /gitlab\.com\/api\/v4\/projects/);
  assert.doesNotMatch(output, /tamux-binaries-/);
});

test("shell installer provisions bundled skills into canonical tamux root", function () {
  const scriptPath = path.join(__dirname, "..", "scripts", "install.sh");
  const script = childProcess.execFileSync("sed", ["-n", "1,380p", scriptPath], {
    cwd: path.join(__dirname, ".."),
    encoding: "utf8",
  });

  assert.match(script, /SKILLS_DIR="\$\{TAMUX_SKILLS_DIR:-\$HOME\/\.tamux\/skills\}"/);
  assert.match(script, /GUIDELINES_DIR="\$\{TAMUX_GUIDELINES_DIR:-\$HOME\/\.tamux\/guidelines\}"/);
  assert.match(script, /Extracting binaries, skills, and guidelines/);
  assert.match(script, /verify_extracted_binaries="\$\{1:-true\}"/);
  assert.match(script, /cp -R "\$EXTRACT_DIR\/skills\/\." "\$SKILLS_DIR\/"/);
  assert.match(script, /install_guidelines\(\)/);
  assert.match(script, /if \[ -e "\$target_path" \]; then/);
});

test("computeSha256Hex returns stable hex digests without shelling out", function () {
  assert.equal(
    computeSha256Hex("tamux"),
    "c013ad25f34616a8c97f18e00b0d33e56e05d8ffe4e1321c6536721e72c10f31"
  );
});

test("shell installer accepts archive-only checksum manifests", { skip: process.platform === "win32" }, function () {
  const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "tamux-install-shell-"));
  const homeDir = path.join(tmpRoot, "home");
  const installDir = path.join(tmpRoot, "bin");
  const skillsDir = path.join(tmpRoot, "skills");
  const guidelinesDir = path.join(tmpRoot, "guidelines");
  const mockBinDir = path.join(tmpRoot, "mock-bin");
  const payloadDir = path.join(tmpRoot, "payload");
  const payloadSkillsDir = path.join(payloadDir, "skills", "demo");
  const payloadGuidelinesDir = path.join(payloadDir, "guidelines");
  fs.mkdirSync(homeDir, { recursive: true });
  fs.mkdirSync(installDir, { recursive: true });
  fs.mkdirSync(skillsDir, { recursive: true });
  fs.mkdirSync(guidelinesDir, { recursive: true });
  fs.mkdirSync(mockBinDir, { recursive: true });
  fs.mkdirSync(payloadSkillsDir, { recursive: true });
  fs.mkdirSync(payloadGuidelinesDir, { recursive: true });

  const binaries = [
    "tamux",
    "tamux-daemon",
    "tamux-tui",
    "tamux-gateway",
    "tamux-mcp",
  ];
  const binaryHashes = new Map();
  for (const name of binaries) {
    const payloadPath = path.join(payloadDir, name);
    const contents = `binary:${name}\n`;
    fs.writeFileSync(payloadPath, contents);
    binaryHashes.set(name, computeSha256Hex(contents));
  }
  fs.writeFileSync(path.join(payloadSkillsDir, "SKILL.md"), "# demo\n");
  fs.writeFileSync(path.join(payloadGuidelinesDir, "coding-task.md"), "# bundled coding\n");
  fs.writeFileSync(path.join(payloadGuidelinesDir, "research-task.md"), "# bundled research\n");
  fs.writeFileSync(path.join(guidelinesDir, "coding-task.md"), "# user coding\n");

  const archivePath = path.join(tmpRoot, "tamux-linux-aarch64.zip");
  const archiveContents = "mock archive payload\n";
  fs.writeFileSync(archivePath, archiveContents);
  const archiveHash = computeSha256Hex(archiveContents);

  fs.writeFileSync(
    path.join(mockBinDir, "curl"),
    `#!/bin/sh
set -eu
output=""
url=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -H)
      shift 2
      ;;
    -o)
      output="$2"
      shift 2
      ;;
    -*)
      shift
      ;;
    *)
      url="$1"
      shift
      ;;
  esac
done

case "$url" in
  *SHA256SUMS-linux-aarch64.txt)
    printf '%s  %s\\n' '${archiveHash}' 'tamux-linux-aarch64.zip' > "$output"
    ;;
  *tamux-linux-aarch64.zip)
    cp '${archivePath}' "$output"
    ;;
  *)
    echo "unexpected mock curl url: $url" >&2
    exit 1
    ;;
esac
`,
    { mode: 0o755 }
  );

  fs.writeFileSync(
    path.join(mockBinDir, "uname"),
    `#!/bin/sh
set -eu
case "\${1:-}" in
  -s) printf 'Linux\\n' ;;
  -m) printf 'aarch64\\n' ;;
  *) /usr/bin/uname "$@" ;;
esac
`,
    { mode: 0o755 }
  );

  fs.writeFileSync(
    path.join(mockBinDir, "unzip"),
    `#!/bin/sh
set -eu
dest=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -d)
      dest="$2"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done

mkdir -p "$dest"
cp -R '${payloadDir}/.' "$dest/"
`,
    { mode: 0o755 }
  );

  const shaCases = binaries
    .map(function (name) {
      return `  *${name}) printf '%s  %s\\n' '${binaryHashes.get(name)}' "$1" ;;`;
    })
    .join("\n");
  fs.writeFileSync(
    path.join(mockBinDir, "sha256sum"),
    `#!/bin/sh
set -eu
case "$1" in
  *tamux-linux-aarch64.zip) printf '%s  %s\\n' '${archiveHash}' "$1" ;;
${shaCases}
  *) echo "unexpected mock sha256sum target: $1" >&2; exit 1 ;;
esac
`,
    { mode: 0o755 }
  );

  const scriptPath = path.join(__dirname, "..", "scripts", "install.sh");
  childProcess.execFileSync("sh", [scriptPath], {
    cwd: path.join(__dirname, ".."),
    env: {
      ...process.env,
      HOME: homeDir,
      PATH: `${mockBinDir}:${process.env.PATH}`,
      TAMUX_VERSION: "0.5.2",
      TAMUX_INSTALL_DIR: installDir,
      TAMUX_SKILLS_DIR: skillsDir,
      TAMUX_GUIDELINES_DIR: guidelinesDir,
    },
    encoding: "utf8",
  });

  for (const name of binaries) {
    assert.ok(fs.existsSync(path.join(installDir, name)), `expected ${name} to be installed`);
  }
  assert.ok(fs.existsSync(path.join(skillsDir, "demo", "SKILL.md")));
  assert.equal(
    fs.readFileSync(path.join(guidelinesDir, "coding-task.md"), "utf8"),
    "# user coding\n"
  );
  assert.equal(
    fs.readFileSync(path.join(guidelinesDir, "research-task.md"), "utf8"),
    "# bundled research\n"
  );
});
