const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const sharedPath = path.join(__dirname, "src/lib/agent-client/shared.ts");
const openAiPath = path.join(__dirname, "src/lib/agent-client/openai.ts");
const sharedSrc = fs.readFileSync(sharedPath, "utf8");
const openAiSrc = fs.readFileSync(openAiPath, "utf8");

test("shared helper defines OpenRouter attribution constants", () => {
  assert.match(sharedSrc, /const OPENROUTER_ATTRIBUTION_URL = "https:\/\/tamux\.app";/);
  assert.match(sharedSrc, /const OPENROUTER_ATTRIBUTION_TITLE = "tamux";/);
  assert.match(sharedSrc, /const OPENROUTER_ATTRIBUTION_CATEGORIES = "cli-agent";/);
  assert.match(sharedSrc, /export function applyOpenRouterAttributionHeaders\(/);
  assert.match(sharedSrc, /headers\["HTTP-Referer"\] = OPENROUTER_ATTRIBUTION_URL;/);
  assert.match(sharedSrc, /headers\["X-OpenRouter-Title"\] = OPENROUTER_ATTRIBUTION_TITLE;/);
  assert.match(sharedSrc, /headers\["X-OpenRouter-Categories"\] = OPENROUTER_ATTRIBUTION_CATEGORIES;/);
});

test("openai client applies OpenRouter attribution to chat and responses requests", () => {
  assert.match(openAiSrc, /applyOpenRouterAttributionHeaders\(req\.provider, headers\);/);
  assert.match(
    openAiSrc,
    /const headers = isSubscription\s*\? buildChatGptCodexHeaders\(req\.config\.api_key, req\._chatgptAccountId\)\s*:\s*\(\(\) => \{[\s\S]*?applyOpenRouterAttributionHeaders\(req\.provider, headers\);[\s\S]*?return headers;[\s\S]*?\}\)\(\);/,
  );
});