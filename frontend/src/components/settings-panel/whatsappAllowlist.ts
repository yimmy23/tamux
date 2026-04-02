import {
    analyzeWhatsAppAllowedContacts,
    hasValidWhatsAppAllowedContacts,
    normalizeWhatsAppPhoneLikeIdentifier,
    parseWhatsAppAllowedContacts,
    WHATSAPP_ALLOWLIST_REQUIRED_MESSAGE,
} from "../../lib/whatsappAllowlist.js";

export {
    analyzeWhatsAppAllowedContacts,
    hasValidWhatsAppAllowedContacts,
    normalizeWhatsAppPhoneLikeIdentifier,
    parseWhatsAppAllowedContacts,
};

export type WhatsAppAllowlistState = {
    contacts: string[];
    invalidEntries: string[];
    hasValidContacts: boolean;
    errorText: string | null;
    warningText: string | null;
    helperText: string;
};

function formatInvalidEntriesMessage(invalidEntries: string[]): string {
    const preview = invalidEntries.slice(0, 3).map((entry) => `"${entry}"`).join(", ");
    const suffix = invalidEntries.length > 3 ? ` and ${invalidEntries.length - 3} more` : "";
    return `Ignored invalid ${invalidEntries.length === 1 ? "entry" : "entries"}: ${preview}${suffix}. Use full phone numbers separated by commas or new lines.`;
}

export function getWhatsAppAllowlistState(raw: string): WhatsAppAllowlistState {
    const analysis = analyzeWhatsAppAllowedContacts(raw);

    if (analysis.validContacts.length === 0) {
        return {
            contacts: analysis.validContacts,
            invalidEntries: analysis.invalidEntries,
            hasValidContacts: false,
            errorText: WHATSAPP_ALLOWLIST_REQUIRED_MESSAGE,
            warningText: analysis.invalidEntries.length > 0 ? formatInvalidEntriesMessage(analysis.invalidEntries) : null,
            helperText: "Separate contacts with commas or new lines. Only listed numbers will be forwarded after connection.",
        };
    }

    return {
        contacts: analysis.validContacts,
        invalidEntries: analysis.invalidEntries,
        hasValidContacts: hasValidWhatsAppAllowedContacts(raw),
        errorText: null,
        warningText: analysis.invalidEntries.length > 0 ? formatInvalidEntriesMessage(analysis.invalidEntries) : null,
        helperText: `${analysis.validContacts.length} allowed contact${analysis.validContacts.length === 1 ? "" : "s"} ready. Separate contacts with commas or new lines.`,
    };
}
