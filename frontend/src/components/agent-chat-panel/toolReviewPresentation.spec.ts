import type { WelesReviewMeta } from "../../lib/agentTools";
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
