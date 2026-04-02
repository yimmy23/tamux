const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { pathToFileURL } = require("node:url");

const mainPath = path.join(__dirname, "main.cjs");
const allowlistPath = path.join(__dirname, "../src/lib/whatsappAllowlist.js");
const linkingPath = path.join(__dirname, "whatsappLinking.js");
const gatewayTabPath = path.join(__dirname, "../src/components/settings-panel/GatewayTab.tsx");
const src = fs.readFileSync(mainPath, "utf8");
const gatewayTabSrc = fs.readFileSync(gatewayTabPath, "utf8");

let desktopAllowlistModulePromise = null;
let desktopLinkingModulePromise = null;

async function loadDesktopAllowlistModule() {
  if (!desktopAllowlistModulePromise) {
    desktopAllowlistModulePromise = import(pathToFileURL(allowlistPath).href);
  }
  return desktopAllowlistModulePromise;
}

async function loadDesktopLinkingModule() {
  if (!desktopLinkingModulePromise) {
    desktopLinkingModulePromise = import(pathToFileURL(linkingPath).href);
  }
  return desktopLinkingModulePromise;
}

test("desktop allowlist helper accepts comma and newline separated contacts", async () => {
  const { parseWhatsAppAllowedContacts } = await loadDesktopAllowlistModule();

  assert.deepEqual(
    parseWhatsAppAllowedContacts(" +15551234567,\n15557654321\n\n +15559876543 "),
    ["15551234567", "15557654321", "15559876543"]
  );
});

test("desktop allowlist helper reports empty allowlist as invalid", async () => {
  const { hasValidWhatsAppAllowedContacts } = await loadDesktopAllowlistModule();

  assert.equal(hasValidWhatsAppAllowedContacts("  , \n \n  "), false);
  assert.equal(hasValidWhatsAppAllowedContacts("+15551234567"), true);
});

test("whatsapp connect refuses to start without allowed contacts", async () => {
  const { assertValidWhatsAppConnectConfig } = await loadDesktopLinkingModule();

  assert.throws(
    () => assertValidWhatsAppConnectConfig({ gateway: { whatsapp_allowed_contacts: "\n,  " } }),
    /Set at least one allowed WhatsApp contact before linking/
  );

  assert.doesNotThrow(() =>
    assertValidWhatsAppConnectConfig({ gateway: { whatsapp_allowed_contacts: "+15551234567" } })
  );
});

test("desktop allowlist analysis reports invalid pasted entries", async () => {
  const { analyzeWhatsAppAllowedContacts } = await loadDesktopAllowlistModule();

  assert.deepEqual(
    analyzeWhatsAppAllowedContacts("+15551234567\ninvalid-user\n(555) 12x\n+15551234567"),
    {
      validContacts: ["15551234567"],
      invalidEntries: ["invalid-user", "(555) 12x"],
      hasValidContacts: true,
    }
  );
});

test("desktop linking helper normalizes renderer QR payload shapes", async () => {
  const { getRendererWhatsAppQrDataUrl } = await loadDesktopLinkingModule();

  assert.equal(getRendererWhatsAppQrDataUrl("data:image/png;base64,abc"), "data:image/png;base64,abc");
  assert.equal(
    getRendererWhatsAppQrDataUrl({ data_url: "data:image/png;base64,xyz", ascii_qr: "ignored" }),
    "data:image/png;base64,xyz"
  );
  assert.equal(getRendererWhatsAppQrDataUrl({ ascii_qr: "text qr only" }), null);
  assert.equal(getRendererWhatsAppQrDataUrl(null), null);
});

test("renderer shows WhatsApp QR data URLs as images", () => {
  assert.match(gatewayTabSrc, /const \[qrDataUrl, setQrDataUrl\]/);
  assert.match(gatewayTabSrc, /<img\s+src=\{qrDataUrl\}/);
  assert.doesNotMatch(gatewayTabSrc, /<pre[\s\S]*qrDataUrl/);
});

test("whatsapp handlers route through daemon agent-bridge commands", () => {
  assert.match(src, /ipcMain\.handle\('whatsapp-connect'[\s\S]*?sendAgentCommand\(\{\s*type:\s*'whats-app-link-start'/);
  assert.match(src, /ensureDaemonWhatsAppSubscribed\(\)[\s\S]*?sendAgentCommand\(\{\s*type:\s*'whats-app-link-subscribe'/);
  assert.match(src, /ipcMain\.handle\('whatsapp-connect'[\s\S]*?ensureDaemonWhatsAppSubscribed\(\)/);
  assert.match(src, /ipcMain\.handle\('whatsapp-disconnect'[\s\S]*?sendAgentCommand\(\{\s*type:\s*'whats-app-link-stop'/);
  assert.match(src, /ipcMain\.handle\('whatsapp-disconnect'[\s\S]*?sendAgentCommand\(\{\s*type:\s*'whats-app-link-unsubscribe'/);
  assert.match(src, /ipcMain\.handle\('whatsapp-status'[\s\S]*?sendAgentQuery\(\{\s*type:\s*'whats-app-link-status'/);
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

test("main resolves shared desktop linking helper via neutral module", () => {
  assert.match(src, /path\.join\(__dirname, 'whatsappLinking\.js'\)/);
  assert.match(src, /getRendererWhatsAppQrDataUrl\(msg\.data\)/);
  assert.doesNotMatch(src, /electron\/whatsappAllowlist/);
});

test("main logs daemon WhatsApp event diagnostics", () => {
  assert.match(src, /logToFile\('info', 'daemon whatsapp-link-status event'/);
  assert.match(src, /logToFile\('info', 'daemon whatsapp-link-qr event'/);
  assert.match(src, /logToFile\('warn', 'daemon whatsapp-link-error event'/);
  assert.match(src, /logToFile\('info', 'daemon whatsapp-link-disconnected event'/);
});

test("agent bridge exit resets daemon WhatsApp subscription state", () => {
  assert.match(src, /bridgeProcess\.on\('exit'[\s\S]*?whatsappDaemonSubscribed\s*=\s*false/);
});

test("agent bridge ready restores daemon WhatsApp subscription when previously desired", () => {
  assert.match(src, /if\s*\(whatsappDaemonSubscriptionDesired\s*&&\s*!whatsappDaemonSubscribed\)[\s\S]*?type:\s*'whats-app-link-subscribe'/);
});

test("whatsapp gateway send is disabled in Electron daemon mode", () => {
  assert.match(
    src,
    /whatsapp-send is disabled in Electron; daemon gateway messaging is authoritative/
  );
});
