const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const mainPath = path.join(__dirname, "main.cjs");
const src = fs.readFileSync(mainPath, "utf8");

test("whatsapp handlers route through daemon agent-bridge commands", () => {
  assert.match(src, /ipcMain\.handle\('whatsapp-connect'[\s\S]*?sendAgentCommand\(\{\s*type:\s*'whatsapp-link-start'/);
  assert.match(src, /ensureDaemonWhatsAppSubscribed\(\)[\s\S]*?sendAgentCommand\(\{\s*type:\s*'whatsapp-link-subscribe'/);
  assert.match(src, /ipcMain\.handle\('whatsapp-connect'[\s\S]*?ensureDaemonWhatsAppSubscribed\(\)/);
  assert.match(src, /ipcMain\.handle\('whatsapp-disconnect'[\s\S]*?sendAgentCommand\(\{\s*type:\s*'whatsapp-link-stop'/);
  assert.match(src, /ipcMain\.handle\('whatsapp-disconnect'[\s\S]*?sendAgentCommand\(\{\s*type:\s*'whatsapp-link-unsubscribe'/);
  assert.match(src, /ipcMain\.handle\('whatsapp-status'[\s\S]*?sendAgentQuery\(\{\s*type:\s*'whatsapp-link-status'/);
});

test("whatsapp handlers check gateway.whatsapp_link_fallback_electron", () => {
  assert.match(src, /whatsapp_link_fallback_electron\s*===\s*true/);
});

test("main forwards daemon WhatsApp link events to renderer channels", () => {
  assert.match(src, /event\.type === 'whatsapp-link-qr'/);
  assert.match(src, /webContents\.send\('whatsapp-qr'/);

  assert.match(src, /event\.type === 'whatsapp-link-linked'/);
  assert.match(src, /webContents\.send\('whatsapp-connected'/);

  assert.match(src, /event\.type === 'whatsapp-link-error'/);
  assert.match(src, /webContents\.send\('whatsapp-error'/);

  assert.match(src, /event\.type === 'whatsapp-link-disconnected'/);
  assert.match(src, /webContents\.send\('whatsapp-disconnected'/);

  assert.match(src, /event\.type === 'whatsapp-link-status'/);
});

test("agent bridge exit resets daemon WhatsApp subscription state", () => {
  assert.match(src, /bridgeProcess\.on\('exit'[\s\S]*?whatsappDaemonSubscribed\s*=\s*false/);
});

test("whatsapp gateway send remains functional in daemon mode", () => {
  assert.match(
    src,
    /WhatsApp send is only available when gateway\.whatsapp_link_fallback_electron is true/
  );
});
