import { getBridge } from "../bridge";
import {
  hasRequiredOperatorProfileFields,
  isOperatorProfileError,
  isOperatorProfileProgress,
  isOperatorProfileQuestion,
  isOperatorProfileSessionCompleted,
  isOperatorProfileSessionStarted,
  parseOperatorProfileSummary,
  type OperatorProfileQuestion,
} from "./operatorProfile";
import type { AgentState, AgentStoreGet, AgentStoreSet } from "./storeTypes";

type OperatorProfileAsyncActionKeys =
  | "startOperatorProfileSession"
  | "fetchNextOperatorProfileQuestion"
  | "submitOperatorProfileAnswer"
  | "skipOperatorProfileQuestion"
  | "deferOperatorProfileQuestion"
  | "getOperatorProfileSummary"
  | "setOperatorProfileConsent"
  | "maybeStartOperatorProfileOnboarding";

function setOperatorProfileLoading(set: AgentStoreSet, loading: boolean, error: string | null = null): void {
  set((state) => ({
    operatorProfile: {
      ...state.operatorProfile,
      loading,
      error,
    },
  }));
}

function setOperatorProfileError(set: AgentStoreSet, error: unknown): void {
  set((state) => ({
    operatorProfile: {
      ...state.operatorProfile,
      loading: false,
      error: error instanceof Error ? error.message : String(error),
    },
  }));
}

function clearOperatorProfileLoading(set: AgentStoreSet): void {
  set((state) => ({
    operatorProfile: {
      ...state.operatorProfile,
      loading: false,
    },
  }));
}

function setOperatorProfileCompleted(set: AgentStoreSet): void {
  set((state) => ({
    operatorProfile: {
      ...state.operatorProfile,
      loading: false,
      panelOpen: false,
      sessionId: null,
      sessionKind: null,
      question: null,
      lastCompletedAt: Date.now(),
    },
  }));
}

function isOperatorProfileAck(value: unknown): value is { ok: true } {
  return Boolean(value && typeof value === "object" && (value as { ok?: unknown }).ok === true);
}

export function createOperatorProfileAsyncActions(
  set: AgentStoreSet,
  get: AgentStoreGet,
): Pick<AgentState, OperatorProfileAsyncActionKeys> {
  let operatorProfileSessionStartPromise: Promise<OperatorProfileQuestion | null> | null = null;
  let operatorProfileOnboardingStartPromise: Promise<void> | null = null;

  return {
    startOperatorProfileSession: async (kind = "first_run_onboarding") => {
      if (operatorProfileSessionStartPromise) {
        return operatorProfileSessionStartPromise;
      }
      const current = get().operatorProfile;
      if (current.loading) {
        return current.question;
      }
      operatorProfileSessionStartPromise = (async () => {
      const bridge = getBridge();
      if (!bridge?.agentStartOperatorProfileSession) {
        setOperatorProfileError(set, "Operator profile bridge not available");
        return null;
      }
      set((state) => ({
        operatorProfile: {
          ...state.operatorProfile,
          panelOpen: true,
          loading: true,
          error: null,
        },
      }));
      try {
        const started = await bridge.agentStartOperatorProfileSession(kind);
        if (isOperatorProfileError(started)) {
          setOperatorProfileLoading(set, false, started.error);
          return null;
        }
        if (!isOperatorProfileSessionStarted(started)) {
          setOperatorProfileLoading(set, false, "Unexpected operator profile session start response");
          return null;
        }
        set((state) => ({
          operatorProfile: {
            ...state.operatorProfile,
            panelOpen: true,
            loading: false,
            error: null,
            sessionId: started.session_id,
            sessionKind: typeof started.kind === "string" ? started.kind : kind,
            question: state.operatorProfile.question?.session_id === started.session_id
              ? state.operatorProfile.question
              : null,
          },
        }));
        const currentQuestion = get().operatorProfile.question;
        if (currentQuestion?.session_id === started.session_id) {
          return currentQuestion;
        }
        return await get().fetchNextOperatorProfileQuestion(started.session_id);
      } catch (error) {
        setOperatorProfileError(set, error);
        return null;
      }
      })();
      try {
        return await operatorProfileSessionStartPromise;
      } finally {
        operatorProfileSessionStartPromise = null;
      }
    },
    fetchNextOperatorProfileQuestion: async (sessionId) => {
      const bridge = getBridge();
      const resolvedSessionId = sessionId ?? get().operatorProfile.sessionId;
      if (!resolvedSessionId || !bridge?.agentNextOperatorProfileQuestion) {
        return null;
      }
      setOperatorProfileLoading(set, true, null);
      try {
        const response = await bridge.agentNextOperatorProfileQuestion(resolvedSessionId);
        if (isOperatorProfileError(response)) {
          setOperatorProfileLoading(set, false, response.error);
          return null;
        }
        if (isOperatorProfileSessionCompleted(response)) {
          setOperatorProfileCompleted(set);
          void get().getOperatorProfileSummary();
          return null;
        }
        if (!isOperatorProfileQuestion(response)) {
          setOperatorProfileLoading(set, false, "Unexpected operator profile question response");
          return null;
        }
        set((state) => ({
          operatorProfile: {
            ...state.operatorProfile,
            loading: false,
            panelOpen: true,
            error: null,
            sessionId: response.session_id,
            question: response,
          },
        }));
        return response;
      } catch (error) {
        setOperatorProfileError(set, error);
        return null;
      }
    },
    submitOperatorProfileAnswer: async (answer) => {
      const bridge = getBridge();
      const state = get().operatorProfile;
      if (!state.sessionId || !state.question || !bridge?.agentSubmitOperatorProfileAnswer) {
        return;
      }
      setOperatorProfileLoading(set, true, null);
      try {
        const response = await bridge.agentSubmitOperatorProfileAnswer(
          state.sessionId,
          state.question.question_id,
          JSON.stringify(answer ?? null),
        );
        if (isOperatorProfileError(response)) {
          setOperatorProfileLoading(set, false, response.error);
          return;
        }
        if (isOperatorProfileSessionCompleted(response)) {
          setOperatorProfileCompleted(set);
          await get().getOperatorProfileSummary();
          return;
        }
        if (isOperatorProfileQuestion(response)) {
          set((current) => ({
            operatorProfile: {
              ...current.operatorProfile,
              loading: false,
              panelOpen: true,
              error: null,
              sessionId: response.session_id,
              question: response,
            },
          }));
          return;
        }
        if (isOperatorProfileAck(response)) {
          clearOperatorProfileLoading(set);
          return;
        }
        if (!isOperatorProfileProgress(response)) {
          setOperatorProfileLoading(set, false, "Unexpected operator profile progress response");
          return;
        }
        set((current) => ({
          operatorProfile: {
            ...current.operatorProfile,
            loading: false,
            progress: response,
          },
        }));
        await get().fetchNextOperatorProfileQuestion(response.session_id);
      } catch (error) {
        setOperatorProfileError(set, error);
      }
    },
    skipOperatorProfileQuestion: async (reason) => {
      const bridge = getBridge();
      const state = get().operatorProfile;
      if (!state.sessionId || !state.question || !bridge?.agentSkipOperatorProfileQuestion) {
        return;
      }
      setOperatorProfileLoading(set, true, null);
      try {
        const response = await bridge.agentSkipOperatorProfileQuestion(
          state.sessionId,
          state.question.question_id,
          reason,
        );
        if (isOperatorProfileError(response)) {
          setOperatorProfileLoading(set, false, response.error);
          return;
        }
        if (isOperatorProfileSessionCompleted(response)) {
          setOperatorProfileCompleted(set);
          await get().getOperatorProfileSummary();
          return;
        }
        if (isOperatorProfileQuestion(response)) {
          set((current) => ({
            operatorProfile: {
              ...current.operatorProfile,
              loading: false,
              panelOpen: true,
              error: null,
              sessionId: response.session_id,
              question: response,
            },
          }));
          return;
        }
        if (isOperatorProfileAck(response)) {
          clearOperatorProfileLoading(set);
          return;
        }
        if (!isOperatorProfileProgress(response)) {
          setOperatorProfileLoading(set, false, "Unexpected operator profile progress response");
          return;
        }
        set((current) => ({
          operatorProfile: {
            ...current.operatorProfile,
            loading: false,
            progress: response,
          },
        }));
        await get().fetchNextOperatorProfileQuestion(response.session_id);
      } catch (error) {
        setOperatorProfileError(set, error);
      }
    },
    deferOperatorProfileQuestion: async (deferUntilUnixMs) => {
      const bridge = getBridge();
      const state = get().operatorProfile;
      if (!state.sessionId || !state.question || !bridge?.agentDeferOperatorProfileQuestion) {
        return;
      }
      setOperatorProfileLoading(set, true, null);
      try {
        const response = await bridge.agentDeferOperatorProfileQuestion(
          state.sessionId,
          state.question.question_id,
          deferUntilUnixMs,
        );
        if (isOperatorProfileError(response)) {
          setOperatorProfileLoading(set, false, response.error);
          return;
        }
        if (isOperatorProfileSessionCompleted(response)) {
          setOperatorProfileCompleted(set);
          await get().getOperatorProfileSummary();
          return;
        }
        if (isOperatorProfileQuestion(response)) {
          set((current) => ({
            operatorProfile: {
              ...current.operatorProfile,
              loading: false,
              panelOpen: true,
              error: null,
              sessionId: response.session_id,
              question: response,
            },
          }));
          return;
        }
        if (isOperatorProfileAck(response)) {
          clearOperatorProfileLoading(set);
          return;
        }
        if (!isOperatorProfileProgress(response)) {
          setOperatorProfileLoading(set, false, "Unexpected operator profile progress response");
          return;
        }
        set((current) => ({
          operatorProfile: {
            ...current.operatorProfile,
            loading: false,
            progress: response,
          },
        }));
        await get().fetchNextOperatorProfileQuestion(response.session_id);
      } catch (error) {
        setOperatorProfileError(set, error);
      }
    },
    getOperatorProfileSummary: async () => {
      const bridge = getBridge();
      if (!bridge?.agentGetOperatorProfileSummary) {
        return null;
      }
      set((state) => ({
        operatorProfile: {
          ...state.operatorProfile,
          loading: state.operatorProfile.panelOpen ? true : state.operatorProfile.loading,
          error: null,
        },
      }));
      try {
        const response = await bridge.agentGetOperatorProfileSummary();
        if (isOperatorProfileError(response)) {
          clearOperatorProfileLoading(set);
          return null;
        }
        const summary = parseOperatorProfileSummary(response);
        if (!summary) {
          clearOperatorProfileLoading(set);
          return null;
        }
        set((state) => ({
          operatorProfile: {
            ...state.operatorProfile,
            loading: false,
            summary,
          },
        }));
        return summary;
      } catch (error) {
        clearOperatorProfileLoading(set);
        return null;
      }
    },
    setOperatorProfileConsent: async (consentKey, granted) => {
      const trimmedKey = consentKey.trim();
      if (!trimmedKey) {
        return false;
      }
      const bridge = getBridge();
      if (!bridge?.agentSetOperatorProfileConsent) {
        setOperatorProfileError(set, "Operator profile bridge not available");
        return false;
      }
      try {
        const result = await bridge.agentSetOperatorProfileConsent(trimmedKey, granted);
        const consentError = typeof result?.error === "string" && result.error.trim().length > 0
          ? result.error
          : null;
        if (consentError) {
          setOperatorProfileLoading(set, false, consentError);
          return false;
        }
        set((state) => {
          const previousSummary = state.operatorProfile.summary;
          if (!previousSummary) {
            return {};
          }
          const nextConsents = [...previousSummary.consents];
          const existingIndex = nextConsents.findIndex((entry) => entry.consent_key === trimmedKey);
          const updatedAt = Date.now();
          if (existingIndex >= 0) {
            nextConsents[existingIndex] = {
              ...nextConsents[existingIndex],
              granted,
              updated_at: updatedAt,
            };
          } else {
            nextConsents.push({
              consent_key: trimmedKey,
              granted,
              updated_at: updatedAt,
            });
          }
          return {
            operatorProfile: {
              ...state.operatorProfile,
              summary: {
                ...previousSummary,
                consents: nextConsents,
              },
            },
          };
        });
        return true;
      } catch (error) {
        setOperatorProfileError(set, error);
        return false;
      }
    },
    maybeStartOperatorProfileOnboarding: async () => {
      if (operatorProfileOnboardingStartPromise) {
        return operatorProfileOnboardingStartPromise;
      }
      operatorProfileOnboardingStartPromise = (async () => {
        const current = get().operatorProfile;
        if (current.loading || current.sessionId || current.question) {
          return;
        }
        const bridge = getBridge();
        if (!bridge?.agentStartOperatorProfileSession || !bridge?.agentGetOperatorProfileSummary) {
          return;
        }
        const summary = await get().getOperatorProfileSummary();
        if (hasRequiredOperatorProfileFields(summary)) {
          set((state) => ({
            operatorProfile: {
              ...state.operatorProfile,
              panelOpen: false,
            },
          }));
          return;
        }
        const next = get().operatorProfile;
        if (next.loading || next.sessionId || next.question) {
          return;
        }
        await get().startOperatorProfileSession("first_run_onboarding");
      })();
      try {
        return await operatorProfileOnboardingStartPromise;
      } finally {
        operatorProfileOnboardingStartPromise = null;
      }
    },
  };
}
