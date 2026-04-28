const { spawn } = require("node:child_process");
const path = require("node:path");

const electronBinary = require("electron");

const child = spawn(electronBinary, ["."], {
    cwd: path.join(__dirname, ".."),
    stdio: "inherit",
    env: {
        ...process.env,
        ZORAI_ELECTRON_USE_DIST_IN_DEV: "1",
    },
});

child.on("error", (error) => {
    console.error("[zorai] Failed to launch Electron:", error?.message || String(error));
    process.exit(1);
});

child.on("exit", (code, signal) => {
    if (signal) {
        process.kill(process.pid, signal);
        return;
    }
    process.exit(code ?? 0);
});
