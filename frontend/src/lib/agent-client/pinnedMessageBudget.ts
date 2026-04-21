import { APPROX_CHARS_PER_TOKEN } from "./types";

export function countUnicodeScalars(text: string): number {
  let count = 0;
  for (const _ of text) {
    count += 1;
  }
  return count;
}

export function pinnedMessageBudgetChars(contextWindowTokens: number): number {
  return Math.floor(Number(contextWindowTokens || 0) * 0.25 * APPROX_CHARS_PER_TOKEN);
}

export function sumMessageContentChars(messages: Array<{ content: string }>): number {
  return messages.reduce((sum, message) => sum + countUnicodeScalars(message.content), 0);
}
