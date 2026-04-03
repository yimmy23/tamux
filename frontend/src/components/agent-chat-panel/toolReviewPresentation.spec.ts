import type { WelesReviewMeta } from "../../lib/agentTools";
import { buildHydratedRemoteThread } from "../../lib/agentStore";
import { parseHandoffSystemEvent } from "./chat-view/helpers";
import {
  buildToolReviewPresentation,
  mergeToolReviewMeta,
} from "./toolReviewPresentation";

function expect(condition: boolean, message: string): void {
  if (!condition) {
    throw new Error(message);
  }
}

const blockedReview: WelesReviewMeta = {
  weles_reviewed: true,
  verdict: "block",
  reasons: ["network access requested"],
  audit_id: "audit-block-1",
};

const flaggedReview: WelesReviewMeta = {
  weles_reviewed: true,
  verdict: "flag_only",
  reasons: ["shell-based Python bypass"],
  security_override_mode: "yolo",
  audit_id: "audit-flag-1",
};

const degradedReview: WelesReviewMeta = {
  weles_reviewed: false,
  verdict: "flag_only",
  reasons: ["WELES unavailable; policy downgraded under yolo"],
  security_override_mode: "yolo",
  audit_id: "audit-degraded-1",
};

const normalPresentation = buildToolReviewPresentation(undefined);
expect(normalPresentation === null, "normal tool calls should render unchanged");

const blockedPresentation = buildToolReviewPresentation(blockedReview);
expect(blockedPresentation?.badgeLabel === "Blocked", "blocked review should render Blocked badge");
expect(blockedPresentation?.tone === "blocked", "blocked review should use blocked tone");
expect(Boolean(blockedPresentation?.reasonText.includes("network access requested")), "blocked review should preserve reasons");

const flaggedPresentation = buildToolReviewPresentation(flaggedReview);
expect(flaggedPresentation?.badgeLabel === "Flagged", "flag_only review should render Flagged badge");
expect(flaggedPresentation?.overrideLabel === "YOLO", "flag_only review should expose yolo override");
expect(flaggedPresentation?.tone === "flagged", "flag_only review should use flagged tone");

const degradedPresentation = buildToolReviewPresentation(degradedReview);
expect(degradedPresentation?.degradedLabel === "Degraded", "unreviewed flagged review should show degraded state");
expect(Boolean(degradedPresentation?.reasonText.includes("WELES unavailable")), "degraded review should preserve fallback rationale");

const mergedReview = mergeToolReviewMeta(blockedReview, flaggedReview);
expect(mergedReview?.verdict === "flag_only", "later tool result review should replace earlier call review");
expect(mergedReview?.security_override_mode === "yolo", "merged review should keep override context");

const internalThread = buildHydratedRemoteThread(
  {
    id: "dm:svarog:weles",
    title: "Internal DM · Swarog ↔ WELES",
    messages: [
      {
        role: "assistant",
        content: '{"verdict":"allow","reasons":["Security level is lowest"]}',
        timestamp: 1,
      },
    ],
  },
  "Svarog",
);
expect(internalThread === null, "internal daemon threads should not hydrate into visible frontend chat state");

const hiddenHandoffThread = buildHydratedRemoteThread(
  {
    id: "handoff:thread-user-1:handoff-1",
    title: "Handoff · Svarog -> Weles",
    messages: [
      {
        role: "system",
        content: "{\"kind\":\"thread_handoff_context\"}",
        timestamp: 1,
      },
    ],
  },
  "Svarog",
);
expect(hiddenHandoffThread === null, "linked handoff threads should stay hidden from the visible frontend chat state");

const visibleThread = buildHydratedRemoteThread(
  {
    id: "thread-user-1",
    title: "Regular Conversation",
    agent_name: "Weles",
    messages: [
      {
        role: "tool",
        content: "Managed command requires approval before execution.",
        tool_name: "bash_command",
        tool_call_id: "call-1",
        tool_status: "error",
        weles_review: flaggedReview,
        timestamp: 2,
      },
    ],
  },
  "Svarog",
);
expect(visibleThread?.thread.title === "Regular Conversation", "non-internal threads should still hydrate normally");
expect(visibleThread?.messages[0]?.welesReview?.verdict === "flag_only", "visible threads should retain weles metadata on tool rows");
expect(visibleThread?.thread.agent_name === "Weles", "hydrated threads should prefer daemon-provided active responder identity");

const handoffSystemEvent = parseHandoffSystemEvent(
  "[[handoff_event]]{\"kind\":\"push\",\"from_agent_name\":\"Svarog\",\"to_agent_name\":\"Weles\",\"reason\":\"Governance review required\",\"summary\":\"Weles should continue answering from here.\"}\nSvarog handed this thread to Weles.",
);
expect(handoffSystemEvent?.to_agent_name === "Weles", "handoff parser should expose the target agent name");
expect(handoffSystemEvent?.summary === "Weles should continue answering from here.", "handoff parser should preserve the collapsed summary payload");
