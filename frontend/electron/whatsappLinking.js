import { assertValidWhatsAppAllowlist } from "../src/lib/whatsappAllowlist.js";

export function getWhatsAppAllowlistRaw(config) {
  return typeof config?.gateway?.whatsapp_allowed_contacts === "string"
    ? config.gateway.whatsapp_allowed_contacts
    : "";
}

export function assertValidWhatsAppConnectConfig(config) {
  assertValidWhatsAppAllowlist(getWhatsAppAllowlistRaw(config));
}

export function getRendererWhatsAppQrDataUrl(payload) {
  if (typeof payload === "string" && payload.trim()) {
    return payload;
  }

  if (typeof payload?.data_url === "string" && payload.data_url.trim()) {
    return payload.data_url;
  }

  return null;
}
