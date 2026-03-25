const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const preloadPath = path.join(__dirname, "preload.cjs");
const mainPath = path.join(__dirname, "main.cjs");
const preloadSrc = fs.readFileSync(preloadPath, "utf8");
const mainSrc = fs.readFileSync(mainPath, "utf8");

test("preload keeps WhatsApp API names stable", () => {
  assert.match(preloadSrc, /whatsappConnect:\s*\(\)\s*=>\s*ipcRenderer\.invoke\('whatsapp-connect'\)/);
  assert.match(preloadSrc, /whatsappDisconnect:\s*\(\)\s*=>\s*ipcRenderer\.invoke\('whatsapp-disconnect'\)/);
  assert.match(preloadSrc, /whatsappStatus:\s*\(\)\s*=>\s*ipcRenderer\.invoke\('whatsapp-status'\)/);
  assert.match(preloadSrc, /whatsappSend:\s*\(jid, text\)\s*=>\s*ipcRenderer\.invoke\('whatsapp-send', jid, text\)/);
});

test("preload exposes daemon-backed WhatsApp event subscriptions", () => {
  assert.match(preloadSrc, /onWhatsAppQR:\s*\(cb\)\s*=>[\s\S]*?ipcRenderer\.on\('whatsapp-qr'/);
  assert.match(preloadSrc, /onWhatsAppConnected:\s*\(cb\)\s*=>[\s\S]*?ipcRenderer\.on\('whatsapp-connected'/);
  assert.match(preloadSrc, /onWhatsAppDisconnected:\s*\(cb\)\s*=>[\s\S]*?ipcRenderer\.on\('whatsapp-disconnected'/);
  assert.match(preloadSrc, /onWhatsAppError:\s*\(cb\)\s*=>[\s\S]*?ipcRenderer\.on\('whatsapp-error'/);
});

test("main uses daemon whatsapp-link protocol command names", () => {
  assert.match(mainSrc, /type:\s*'whatsapp-link-start'/);
  assert.match(mainSrc, /type:\s*'whatsapp-link-stop'/);
  assert.match(mainSrc, /type:\s*'whatsapp-link-status'/);
  assert.match(mainSrc, /type:\s*'whatsapp-link-subscribe'/);
  assert.match(mainSrc, /type:\s*'whatsapp-link-unsubscribe'/);
});
