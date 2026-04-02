export const WHATSAPP_ALLOWLIST_REQUIRED_MESSAGE: string;

export function normalizeWhatsAppPhoneLikeIdentifier(raw: string): string | null;
export function analyzeWhatsAppAllowedContacts(raw: string): {
  validContacts: string[];
  invalidEntries: string[];
  hasValidContacts: boolean;
};
export function parseWhatsAppAllowedContacts(raw: string): string[];
export function hasValidWhatsAppAllowedContacts(raw: string): boolean;
export function assertValidWhatsAppAllowlist(raw: string): void;
