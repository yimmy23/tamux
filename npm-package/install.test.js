"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");

const install = require("./install");

test("getReleaseAssetInfo maps linux x64 to published zip asset names", function () {
  const info = install.getReleaseAssetInfo("linux", "x64", "0.2.0");

  assert.deepEqual(info, {
    archiveName: "tamux-linux-x86_64.zip",
    checksumName: "SHA256SUMS-linux-x86_64.txt",
    bundleChecksumName: "SHA256SUMS.txt",
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