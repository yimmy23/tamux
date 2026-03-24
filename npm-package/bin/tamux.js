#!/usr/bin/env node
// bin/tamux.js -- launcher wrapper that spawns the native tamux binary.
// Falls back to downloading binaries if they are missing (two-layer fallback).
"use strict";

var fs = require("fs");
var path = require("path");
var child_process = require("child_process");

var binaryName = process.platform === "win32" ? "tamux.exe" : "tamux";
var binaryPath = path.join(__dirname, binaryName);

/**
 * Attempt to download binaries using the postinstall script.
 * This is the second layer of the fallback (first is postinstall during npm install).
 */
function tryFallbackDownload() {
  console.log("tamux: binary not found, attempting download...");
  try {
    require("../install"); // side-effect: triggers async download
    // install() returns a promise -- we need to wait synchronously here
    // since the module's top-level main().catch() already runs on require.
    // After require completes, the download has been initiated.
    // Give the async download a moment by checking existence in a callback.
  } catch (err) {
    console.error(
      "tamux: fallback download failed: " + err.message
    );
  }
}

/**
 * Spawn the native binary with all CLI arguments forwarded.
 * @param {string} binPath
 */
function spawnBinary(binPath) {
  var child = child_process.spawn(binPath, process.argv.slice(2), {
    stdio: "inherit",
  });

  // Forward signals to the child process
  process.on("SIGINT", function () {
    child.kill("SIGINT");
  });
  process.on("SIGTERM", function () {
    child.kill("SIGTERM");
  });

  child.on("error", function (err) {
    console.error("tamux: failed to start binary: " + err.message);
    process.exit(1);
  });

  child.on("close", function (code) {
    process.exit(code !== null ? code : 1);
  });
}

// Main logic: find binary or download it
if (fs.existsSync(binaryPath)) {
  spawnBinary(binaryPath);
} else {
  // Binary not found -- attempt fallback download
  tryFallbackDownload();

  // The install module runs async; use a short poll to wait for the binary
  // to appear (up to 60 seconds). This covers the case where postinstall
  // was skipped (e.g., --ignore-scripts) and we need a runtime download.
  var attempts = 0;
  var maxAttempts = 120;
  var pollInterval = 500; // ms

  var timer = setInterval(function () {
    attempts++;
    if (fs.existsSync(binaryPath)) {
      clearInterval(timer);
      spawnBinary(binaryPath);
    } else if (attempts >= maxAttempts) {
      clearInterval(timer);
      console.error(
        "tamux: could not download binary for your platform. " +
          "Visit https://gitlab.com/anthropic/tamux/-/releases for manual download."
      );
      process.exit(1);
    }
  }, pollInterval);
}
