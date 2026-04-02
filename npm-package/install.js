#!/usr/bin/env node
// install.js -- download platform-specific tamux binaries on npm install
"use strict";

const https = require("https");
const http = require("http");
const fs = require("fs");
const path = require("path");
const os = require("os");
const crypto = require("crypto");
const { execFileSync } = require("child_process");

const VERSION = require("./package.json").version;
const BIN_DIR = path.join(__dirname, "bin");

// GitHub owner/repo for release asset downloads.
const GITHUB_OWNER = "mkurman";
const GITHUB_REPO = "tamux";

const PLATFORM_MAP = {
  "linux-x64": "linux-x64",
  "linux-arm64": "linux-arm64",
  "darwin-arm64": "darwin-arm64",
  "darwin-x64": "darwin-x64",
  "win32-x64": "windows-x64",
};

const BASE_URL = `https://github.com/${GITHUB_OWNER}/${GITHUB_REPO}/releases/download/v${VERSION}`;

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

async function main() {
  var key = os.platform() + "-" + os.arch();
  var target = PLATFORM_MAP[key];

  // --dry-run: print platform detection info and exit without downloading
  if (process.argv.includes("--dry-run")) {
    console.log("tamux install.js dry-run");
    console.log("  platform key: " + key);
    console.log("  target:       " + (target || "unsupported"));
    console.log("  version:      " + VERSION);
    console.log("  bin dir:      " + BIN_DIR);
    if (target) {
      var tarball = "tamux-binaries-v" + VERSION + "-" + target + ".tar.gz";
      console.log("  download URL: " + BASE_URL + "/" + tarball);
      console.log("  checksum URL: " + BASE_URL + "/SHA256SUMS-" + target + ".txt");
    }
    process.exit(0);
  }

  if (!target) {
    console.warn(
      "tamux: unsupported platform " + key + ", skipping binary download"
    );
    process.exit(0);
  }

  var tarballName =
    "tamux-binaries-v" + VERSION + "-" + target + ".tar.gz";
  var checksumsName = "SHA256SUMS-" + target + ".txt";
  var tarballUrl = BASE_URL + "/" + tarballName;
  var checksumsUrl = BASE_URL + "/" + checksumsName;
  var tmpPath = path.join(os.tmpdir(), tarballName);

  // 1. Ensure bin directory exists
  fs.mkdirSync(BIN_DIR, { recursive: true });

  // 2. Download SHA256SUMS file
  console.log("tamux: downloading checksums...");
  var checksumsData = await download(checksumsUrl);
  var expectedHash = parseChecksumFile(checksumsData, tarballName);
  if (!expectedHash) {
    console.warn(
      "tamux: could not find checksum for " +
        tarballName +
        " in SHA256SUMS, skipping verification"
    );
  }

  // 3. Download tarball
  console.log("tamux: downloading binaries for " + target + "...");
  var tarballData = await download(tarballUrl);
  fs.writeFileSync(tmpPath, tarballData);

  // 4. Verify SHA256 checksum
  if (expectedHash) {
    console.log("tamux: verifying SHA256 checksum...");
    var valid = await verifyChecksum(tmpPath, expectedHash);
    if (!valid) {
      console.warn("tamux: SHA256 checksum mismatch, skipping extraction");
      try {
        fs.unlinkSync(tmpPath);
      } catch (_e) {
        /* ignore cleanup errors */
      }
      process.exit(0);
    }
    console.log("tamux: checksum OK");
  }

  // 5. Extract tarball
  console.log("tamux: extracting binaries...");
  execFileSync("tar", ["xzf", tmpPath, "-C", BIN_DIR]);

  // 6. Set executable permissions (Unix only)
  if (os.platform() !== "win32") {
    var binaries = ["tamux-daemon", "tamux", "tamux-tui"];
    for (var i = 0; i < binaries.length; i++) {
      var binPath = path.join(BIN_DIR, binaries[i]);
      if (fs.existsSync(binPath)) {
        fs.chmodSync(binPath, 0o755);
      }
    }
  }

  // 7. Clean up temp file
  try {
    fs.unlinkSync(tmpPath);
  } catch (_e) {
    /* ignore cleanup errors */
  }

  console.log("tamux: installation complete");
}

module.exports = main;
module.exports.GITHUB_OWNER = GITHUB_OWNER;
module.exports.GITHUB_REPO = GITHUB_REPO;

// Auto-run only when executed directly (postinstall) or via tryFallbackDownload,
// not when required just for the exported constants.
if (require.main === module) {
  main().catch(function (err) {
    console.warn("tamux: postinstall binary download failed: " + err.message);
    console.warn("tamux: binaries will be downloaded on first run");
    process.exit(0);
  });
}
