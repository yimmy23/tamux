import { APPROX_CHARS_PER_TOKEN } from "./types";

export function countUnicodeScalars(text: string): number {
  return Array.from(text).length;
}

export function pinnedMessageBudgetChars(contextWindowTokens: number): number {
  return Math.floor(Number(contextWindowTokens || 0) * 0.25 * APPROX_CHARS_PER_TOKEN);
}

export function sumMessageContentChars(messages: Array<{ content: string }>): number {
  return messages.reduce((sum, message) => sum + countUnicodeScalars(message.content), 0);
}
