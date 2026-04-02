import {
  isOperatorProfileProgress,
  isOperatorProfileQuestion,
  type OperatorProfileSessionCompleted,
  type OperatorProfileSessionStarted,
} from "./operatorProfile";
import type { AgentState, AgentStoreSet } from "./storeTypes";

type OperatorProfileStateActionKeys =
  | "setOperatorProfilePanelOpen"
  | "applyOperatorProfileSessionStarted"
  | "applyOperatorProfileQuestion"
  | "applyOperatorProfileProgress"
  | "applyOperatorProfileSessionCompleted";

export function createOperatorProfileStateActions(
  set: AgentStoreSet,
): Pick<AgentState, OperatorProfileStateActionKeys> {
  return {
    setOperatorProfilePanelOpen: (open) => {
      set((state) => ({
        operatorProfile: {
          ...state.operatorProfile,
          panelOpen: open,
        },
      }));
    },
    applyOperatorProfileSessionStarted: (event: OperatorProfileSessionStarted) => {
      if (!event?.session_id) {
        return;
      }
      set((state) => ({
        operatorProfile: {
          ...state.operatorProfile,
          panelOpen: true,
          loading: false,
          error: null,
          sessionId: event.session_id,
          sessionKind: typeof event.kind === "string" ? event.kind : state.operatorProfile.sessionKind,
        },
      }));
    },
    applyOperatorProfileQuestion: (question) => {
      if (!isOperatorProfileQuestion(question)) {
        return;
      }
      set((state) => ({
        operatorProfile: {
          ...state.operatorProfile,
          panelOpen: true,
          loading: false,
          error: null,
          sessionId: question.session_id,
          question,
        },
      }));
    },
    applyOperatorProfileProgress: (progress) => {
      if (!isOperatorProfileProgress(progress)) {
        return;
      }
      set((state) => ({
        operatorProfile: {
          ...state.operatorProfile,
          loading: false,
          error: null,
          sessionId: progress.session_id,
          progress,
        },
      }));
    },
    applyOperatorProfileSessionCompleted: (completed: OperatorProfileSessionCompleted) => {
      if (!completed?.session_id) {
        return;
      }
      set((state) => ({
        operatorProfile: {
          ...state.operatorProfile,
          panelOpen: false,
          loading: false,
          error: null,
          sessionId: null,
          sessionKind: null,
          question: null,
          lastCompletedAt: Date.now(),
        },
      }));
    },
  };
}
