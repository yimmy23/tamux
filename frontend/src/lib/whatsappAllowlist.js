export const WHATSAPP_ALLOWLIST_REQUIRED_MESSAGE = "Set at least one allowed WhatsApp contact before linking.";

function splitWhatsAppAllowedContacts(raw) {
  return String(raw || "")
    .split(/[\n,]/)
    .map((entry) => entry.trim())
    .filter(Boolean);
}

export function normalizeWhatsAppPhoneLikeIdentifier(raw) {
  if (typeof raw !== "string") {
    return null;
  }

  const trimmed = raw.trim();
  if (!trimmed) {
    return null;
  }

  const candidate = trimmed.endsWith("@s.whatsapp.net")
    ? trimmed.slice(0, -"@s.whatsapp.net".length)
    : trimmed.endsWith("@c.us")
      ? trimmed.slice(0, -"@c.us".length)
      : trimmed.includes("@")
        ? null
        : trimmed;

  if (!candidate) {
    return null;
  }

  let index = candidate.startsWith("+") ? 1 : 0;
  let sawGroup = false;

  while (index < candidate.length) {
    let digitCount = 0;

    if (candidate[index] === "(") {
      index += 1;
      while (index < candidate.length && /\d/.test(candidate[index])) {
        index += 1;
        digitCount += 1;
      }

      if (digitCount === 0 || candidate[index] !== ")") {
        return null;
      }

      index += 1;
    } else {
      while (index < candidate.length && /\d/.test(candidate[index])) {
        index += 1;
        digitCount += 1;
      }

      if (digitCount === 0) {
        return null;
      }
    }

    sawGroup = true;

    if (index >= candidate.length) {
      break;
    }

    const separator = candidate[index];
    if (separator !== " " && separator !== "-") {
      return null;
    }

    index += 1;
    if (index >= candidate.length || candidate[index] === " " || candidate[index] === "-") {
      return null;
    }
  }

  if (!sawGroup) {
    return null;
  }

  const digits = Array.from(candidate).filter((char) => /\d/.test(char)).join("");
  return digits || null;
}

export function analyzeWhatsAppAllowedContacts(raw) {
  const validContacts = [];
  const invalidEntries = [];
  const seenValidContacts = new Set();
  const seenInvalidEntries = new Set();

  for (const entry of splitWhatsAppAllowedContacts(raw)) {
    const normalized = normalizeWhatsAppPhoneLikeIdentifier(entry);
    if (!normalized) {
      if (!seenInvalidEntries.has(entry)) {
        seenInvalidEntries.add(entry);
        invalidEntries.push(entry);
      }
      continue;
    }

    if (seenValidContacts.has(normalized)) {
      continue;
    }

    seenValidContacts.add(normalized);
    validContacts.push(normalized);
  }

  return {
    validContacts,
    invalidEntries,
    hasValidContacts: validContacts.length > 0,
  };
}

export function parseWhatsAppAllowedContacts(raw) {
  return analyzeWhatsAppAllowedContacts(raw).validContacts;
}

export function hasValidWhatsAppAllowedContacts(raw) {
  return analyzeWhatsAppAllowedContacts(raw).hasValidContacts;
}

export function assertValidWhatsAppAllowlist(raw) {
  if (!hasValidWhatsAppAllowedContacts(raw)) {
    throw new Error(`${WHATSAPP_ALLOWLIST_REQUIRED_MESSAGE} Separate contacts with commas or new lines.`);
  }
}
