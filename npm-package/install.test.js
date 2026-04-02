"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");

const install = require("./install");

test("getReleaseAssetInfo maps linux x64 to published zip asset names", function () {
  const info = install.getReleaseAssetInfo("linux", "x64", "0.2.0");

  assert.deepEqual(info, {
    archiveName: "tamux-0.2.0-linux-x86_64.zip",
    checksumName: "SHA256SUMS-linux-x86_64.txt",
    bundleChecksumName: "SHA256SUMS.txt",
    requiredBinaries: ["tamux", "tamux-daemon", "tamux-tui"],
  });
});

test("getReleaseAssetInfo maps windows x64 to published zip asset names", function () {
  const info = install.getReleaseAssetInfo("win32", "x64", "0.2.0");

  assert.deepEqual(info, {
    archiveName: "tamux-0.2.0-windows-x64.zip",
    checksumName: "SHA256SUMS-windows-x64.txt",
    bundleChecksumName: "SHA256SUMS.txt",
    requiredBinaries: ["tamux.exe", "tamux-daemon.exe", "tamux-tui.exe"],
  });
});

test("getReleaseAssetInfo returns null for unsupported targets", function () {
  assert.equal(install.getReleaseAssetInfo("linux", "ppc64"), null);
});