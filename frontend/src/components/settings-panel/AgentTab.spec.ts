import { normalizeLlmStreamTimeoutInput } from "./agentTabHelpers";

function assert(condition: unknown, message: string): void {
    if (!condition) {
        throw new Error(message);
    }
}

assert(
    normalizeLlmStreamTimeoutInput("30.5") === 30,
    "Decimal timeout input should coerce to an in-range integer",
);

assert(
    normalizeLlmStreamTimeoutInput("9999") === 1800,
    "Timeout input should clamp to the maximum allowed value",
);

assert(
    normalizeLlmStreamTimeoutInput("29") === 30,
    "Timeout input should clamp to the minimum allowed value",
);
