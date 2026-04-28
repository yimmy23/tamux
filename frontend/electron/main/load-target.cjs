function shouldUseBuiltDistInDev(env = {}) {
    return String(env.ZORAI_ELECTRON_USE_DIST_IN_DEV ?? "").trim() === "1";
}

function resolveRendererLoadTarget(options) {
    const {
        app,
        electronDir,
        env = process.env,
        path,
    } = options;

    if (app?.isPackaged || shouldUseBuiltDistInDev(env)) {
        return {
            kind: "file",
            value: path.join(electronDir, "..", "dist", "index.html"),
        };
    }

    return {
        kind: "url",
        value: "http://localhost:5173",
    };
}

module.exports = {
    resolveRendererLoadTarget,
    shouldUseBuiltDistInDev,
};
