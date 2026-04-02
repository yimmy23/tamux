const INLINE_APPROVAL_PROMPT_RE = [
  /trust(?:ed)?\s+(?:this|the)\s+(?:folder|directory|workspace|project)/i,
  /trust\s+the\s+files?\s+in\s+this\s+folder/i,
  /\bdo\s+you\s+approve\b/i,
];

const INLINE_APPROVAL_RESPONSE_HINT_RE = [
  /\(\s*[yY]\s*\/\s*[nN]\s*\)/,
  /\[\s*[yY]\s*\/\s*[nN]\s*\]/,
  /\b(?:yes|no)\b/i,
];

export function detectInlineApprovalPrompt(buffer: string): string | null {
  const normalized = buffer.replace(/\r/g, "\n");
  const lines = normalized
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .slice(-8);

  for (let index = lines.length - 1; index >= 0; index -= 1) {
    const line = lines[index];
    const looksLikeApproval = INLINE_APPROVAL_PROMPT_RE.some((pattern) => pattern.test(line));
    if (!looksLikeApproval) continue;
    const hasResponseHint = INLINE_APPROVAL_RESPONSE_HINT_RE.some((pattern) => pattern.test(line));
    if (hasResponseHint || /[?]$/.test(line)) {
      return line;
    }
  }

  return null;
}
