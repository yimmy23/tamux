import { beforeEach, expect, test, vi } from "vitest";
import {
  DEFAULT_OPERATOR_PROFILE_STATE,
  isOperatorProfileSessionCompleted,
  normalizeOperatorProfileInputKind,
} from "./operatorProfile";
import { useAgentStore } from "./store";

beforeEach(() => {
  useAgentStore.setState({
    operatorProfile: DEFAULT_OPERATOR_PROFILE_STATE,
  });
  vi.restoreAllMocks();
  Reflect.deleteProperty(globalThis, "window");
});

test("normalizeOperatorProfileInputKind treats daemon boolean input as boolean UI input", () => {
  expect(normalizeOperatorProfileInputKind("boolean")).toBe("bool");
  expect(normalizeOperatorProfileInputKind("bool")).toBe("bool");
});

test("normalizeOperatorProfileInputKind keeps unknown input kinds as text fallback", () => {
  expect(normalizeOperatorProfileInputKind("")).toBe("text");
  expect(normalizeOperatorProfileInputKind("unknown")).toBe("text");
});

test("operator profile completion guard does not match question payloads", () => {
  expect(isOperatorProfileSessionCompleted({
    session_id: "ops-1",
    question_id: "enabled",
    field_key: "enabled",
    prompt: "Enable operator modeling overall?",
    input_kind: "boolean",
    optional: false,
  })).toBe(false);
  expect(isOperatorProfileSessionCompleted({
    session_id: "ops-1",
    updated_fields: ["enabled"],
  })).toBe(true);
});

test("startOperatorProfileSession ignores duplicate calls while a start is pending", async () => {
  const agentStartOperatorProfileSession = vi.fn(async () => ({
    session_id: "ops-1",
    kind: "first_run_onboarding",
  }));
  const agentNextOperatorProfileQuestion = vi.fn(async () => ({
    session_id: "ops-1",
    question_id: "enabled",
    field_key: "enabled",
    prompt: "Enable operator modeling overall?",
    input_kind: "boolean",
    optional: false,
  }));

  Object.assign(globalThis, {
    window: {
      zorai: {
        agentStartOperatorProfileSession,
        agentNextOperatorProfileQuestion,
      },
    },
  });

  const first = useAgentStore.getState().startOperatorProfileSession("first_run_onboarding");
  const second = useAgentStore.getState().startOperatorProfileSession("first_run_onboarding");

  expect(agentStartOperatorProfileSession).toHaveBeenCalledTimes(1);
  await expect(first).resolves.toMatchObject({ question_id: "enabled" });
  await expect(second).resolves.toMatchObject({ question_id: "enabled" });
});

test("startOperatorProfileSession stays deduped if loading is cleared before bridge resolves", async () => {
  let resolveStarted: ((value: { session_id: string; kind: string }) => void) | null = null;
  const agentStartOperatorProfileSession = vi.fn(() => new Promise<{ session_id: string; kind: string }>((resolve) => {
    resolveStarted = resolve;
  }));
  const agentNextOperatorProfileQuestion = vi.fn(async () => ({
    session_id: "ops-1",
    question_id: "enabled",
    field_key: "enabled",
    prompt: "Enable operator modeling overall?",
    input_kind: "boolean",
    optional: false,
  }));

  Object.assign(globalThis, {
    window: {
      zorai: {
        agentStartOperatorProfileSession,
        agentNextOperatorProfileQuestion,
      },
    },
  });

  const first = useAgentStore.getState().startOperatorProfileSession("first_run_onboarding");
  useAgentStore.setState((state) => ({
    operatorProfile: {
      ...state.operatorProfile,
      loading: false,
    },
  }));
  const second = useAgentStore.getState().startOperatorProfileSession("first_run_onboarding");

  expect(agentStartOperatorProfileSession).toHaveBeenCalledTimes(1);
  resolveStarted?.({ session_id: "ops-1", kind: "first_run_onboarding" });
  await expect(first).resolves.toMatchObject({ question_id: "enabled" });
  await expect(second).resolves.toMatchObject({ question_id: "enabled" });
});

test("maybeStartOperatorProfileOnboarding does not surface optional summary timeouts", async () => {
  const agentGetOperatorProfileSummary = vi.fn(async () => ({
    error: "Agent query timeout: operator-profile-summary",
  }));
  const agentStartOperatorProfileSession = vi.fn(async () => ({
    session_id: "ops-1",
    kind: "first_run_onboarding",
  }));
  const agentNextOperatorProfileQuestion = vi.fn(async () => ({
    session_id: "ops-1",
    question_id: "enabled",
    field_key: "enabled",
    prompt: "Enable operator modeling overall?",
    input_kind: "boolean",
    optional: false,
  }));

  Object.assign(globalThis, {
    window: {
      zorai: {
        agentGetOperatorProfileSummary,
        agentStartOperatorProfileSession,
        agentNextOperatorProfileQuestion,
      },
    },
  });

  await useAgentStore.getState().maybeStartOperatorProfileOnboarding();

  expect(agentStartOperatorProfileSession).toHaveBeenCalledTimes(1);
  expect(useAgentStore.getState().operatorProfile.error).toBeNull();
  expect(useAgentStore.getState().operatorProfile.question?.question_id).toBe("enabled");
});

test("maybeStartOperatorProfileOnboarding coalesces concurrent welcome-triggered starts", async () => {
  let resolveSummary: ((value: {
    summary_json: string;
  }) => void) | null = null;
  const agentGetOperatorProfileSummary = vi.fn(() => new Promise<{ summary_json: string }>((resolve) => {
    resolveSummary = resolve;
  }));
  const agentStartOperatorProfileSession = vi.fn(async () => ({
    session_id: "ops-1",
    kind: "first_run_onboarding",
  }));
  const agentNextOperatorProfileQuestion = vi.fn(async () => ({
    session_id: "ops-1",
    question_id: "enabled",
    field_key: "enabled",
    prompt: "Enable operator modeling overall?",
    input_kind: "boolean",
    optional: false,
  }));

  Object.assign(globalThis, {
    window: {
      zorai: {
        agentGetOperatorProfileSummary,
        agentStartOperatorProfileSession,
        agentNextOperatorProfileQuestion,
      },
    },
  });

  const first = useAgentStore.getState().maybeStartOperatorProfileOnboarding();
  const second = useAgentStore.getState().maybeStartOperatorProfileOnboarding();
  const third = useAgentStore.getState().maybeStartOperatorProfileOnboarding();

  expect(agentGetOperatorProfileSummary).toHaveBeenCalledTimes(1);
  resolveSummary?.({
    summary_json: JSON.stringify({
      model: {},
      consents: [],
    }),
  });
  await Promise.all([first, second, third]);

  expect(agentStartOperatorProfileSession).toHaveBeenCalledTimes(1);
  expect(agentNextOperatorProfileQuestion).toHaveBeenCalledTimes(1);
});

test("getOperatorProfileSummary keeps summary timeouts out of onboarding panel errors", async () => {
  const agentGetOperatorProfileSummary = vi.fn(async () => ({
    error: "Agent query timeout: operator-profile-summary",
  }));

  Object.assign(globalThis, {
    window: {
      zorai: {
        agentGetOperatorProfileSummary,
      },
    },
  });
  useAgentStore.setState((state) => ({
    operatorProfile: {
      ...state.operatorProfile,
      panelOpen: true,
    },
  }));

  await expect(useAgentStore.getState().getOperatorProfileSummary()).resolves.toBeNull();

  expect(useAgentStore.getState().operatorProfile.error).toBeNull();
  expect(useAgentStore.getState().operatorProfile.loading).toBe(false);
});

test("submitOperatorProfileAnswer clears saving state after command ack", async () => {
  const agentSubmitOperatorProfileAnswer = vi.fn(async () => ({ ok: true }));
  Object.assign(globalThis, {
    window: {
      zorai: {
        agentSubmitOperatorProfileAnswer,
      },
    },
  });
  useAgentStore.setState((state) => ({
    operatorProfile: {
      ...state.operatorProfile,
      sessionId: "ops-1",
      question: {
        session_id: "ops-1",
        question_id: "enabled",
        field_key: "enabled",
        prompt: "Enable operator modeling overall?",
        input_kind: "boolean",
        optional: false,
      },
    },
  }));

  await useAgentStore.getState().submitOperatorProfileAnswer(true);

  expect(agentSubmitOperatorProfileAnswer).toHaveBeenCalledWith("ops-1", "enabled", "true");
  expect(useAgentStore.getState().operatorProfile.loading).toBe(false);
  expect(useAgentStore.getState().operatorProfile.error).toBeNull();
});
