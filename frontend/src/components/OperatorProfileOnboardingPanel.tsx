import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { useAgentStore } from "../lib/agentStore";

const SELECT_OPTIONS_BY_FIELD: Record<string, string[]> = {
  notification_preference: ["minimal", "balanced", "proactive"],
};

function getQuestionSelectOptions(fieldKey: string): string[] {
  return SELECT_OPTIONS_BY_FIELD[fieldKey] ?? [];
}

export function OperatorProfileOnboardingPanel() {
  const operatorProfile = useAgentStore((s) => s.operatorProfile);
  const fetchNextQuestion = useAgentStore((s) => s.fetchNextOperatorProfileQuestion);
  const submitAnswer = useAgentStore((s) => s.submitOperatorProfileAnswer);
  const skipQuestion = useAgentStore((s) => s.skipOperatorProfileQuestion);
  const deferQuestion = useAgentStore((s) => s.deferOperatorProfileQuestion);
  const setPanelOpen = useAgentStore((s) => s.setOperatorProfilePanelOpen);

  const question = operatorProfile.question;
  const inputKind = question?.input_kind ?? "text";
  const selectOptions = useMemo(
    () => getQuestionSelectOptions(question?.field_key ?? ""),
    [question?.field_key],
  );

  const [textValue, setTextValue] = useState("");
  const [boolValue, setBoolValue] = useState<boolean | null>(null);
  const [selectValue, setSelectValue] = useState("");

  useEffect(() => {
    setTextValue("");
    setBoolValue(null);
    setSelectValue(selectOptions[0] ?? "");
  }, [question?.question_id, selectOptions]);

  if (!operatorProfile.panelOpen) {
    return null;
  }

  const answered = operatorProfile.progress?.answered ?? 0;
  const remaining = operatorProfile.progress?.remaining ?? (question ? 1 : 0);
  const total = Math.max(1, answered + remaining);
  const completionRatio = operatorProfile.progress?.completion_ratio ?? answered / total;
  const completionPct = Math.max(0, Math.min(100, Math.round(completionRatio * 100)));

  const canSubmit = Boolean(question) && (
    (inputKind === "text" && textValue.trim().length > 0)
    || (inputKind === "bool" && typeof boolValue === "boolean")
    || (inputKind === "select" && selectValue.trim().length > 0)
    || (inputKind !== "text" && inputKind !== "bool" && inputKind !== "select")
  );

  const submitValue = async () => {
    if (!question || !canSubmit) {
      return;
    }
    if (inputKind === "bool") {
      await submitAnswer(boolValue);
      return;
    }
    if (inputKind === "select") {
      await submitAnswer(selectValue);
      return;
    }
    await submitAnswer(textValue.trim());
  };

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 4300,
        background: "rgba(3, 8, 16, 0.74)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: 20,
      }}
    >
      <div
        style={{
          width: "min(760px, 96vw)",
          border: "1px solid rgba(255,255,255,0.14)",
          background: "linear-gradient(180deg, rgba(10, 17, 28, 0.98), rgba(10, 16, 24, 0.96))",
          boxShadow: "0 24px 80px rgba(0,0,0,0.45)",
          padding: 18,
          display: "grid",
          gap: 14,
        }}
      >
        <div style={{ display: "flex", justifyContent: "space-between", gap: 12, alignItems: "start" }}>
          <div style={{ display: "grid", gap: 4 }}>
            <span style={{ fontSize: 12, letterSpacing: "0.08em", textTransform: "uppercase", color: "var(--text-secondary)" }}>
              About You
            </span>
            <h2 style={{ margin: 0, fontSize: 22 }}>Operator Profile Onboarding</h2>
            <span style={{ fontSize: 12, color: "var(--text-secondary)" }}>
              {operatorProfile.sessionKind ?? "first_run_onboarding"}
            </span>
          </div>
          <button
            type="button"
            onClick={() => setPanelOpen(false)}
            style={secondaryButtonStyle}
          >
            Hide
          </button>
        </div>

        <div style={{ display: "grid", gap: 6 }}>
          <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12, color: "var(--text-secondary)" }}>
            <span>Progress</span>
            <span>{answered} answered • {remaining} remaining</span>
          </div>
          <div style={{ height: 8, background: "rgba(255,255,255,0.12)", overflow: "hidden" }}>
            <div
              style={{
                width: `${completionPct}%`,
                height: "100%",
                background: "var(--accent)",
                transition: "width 140ms ease",
              }}
            />
          </div>
        </div>

        {!question ? (
          <div style={{ fontSize: 13, color: "var(--text-secondary)", padding: "4px 0" }}>
            {operatorProfile.loading ? "Loading your next question..." : "No pending profile question."}
          </div>
        ) : (
          <div style={{ display: "grid", gap: 10 }}>
            <div style={{ fontSize: 16, lineHeight: 1.45 }}>{question.prompt}</div>
            <div style={{ fontSize: 12, color: "var(--text-secondary)" }}>
              Field: <code>{question.field_key}</code> • {question.optional ? "optional" : "recommended"}
            </div>

            {inputKind === "bool" ? (
              <div style={{ display: "flex", gap: 8 }}>
                <button
                  type="button"
                  onClick={() => setBoolValue(true)}
                  style={boolValue === true ? selectedButtonStyle : secondaryButtonStyle}
                >
                  Yes
                </button>
                <button
                  type="button"
                  onClick={() => setBoolValue(false)}
                  style={boolValue === false ? selectedButtonStyle : secondaryButtonStyle}
                >
                  No
                </button>
              </div>
            ) : null}

            {inputKind === "select" ? (
              <select
                value={selectValue}
                onChange={(event) => setSelectValue(event.target.value)}
                style={inputStyle}
              >
                {selectOptions.length > 0 ? (
                  selectOptions.map((option) => (
                    <option key={option} value={option}>{option}</option>
                  ))
                ) : (
                  <option value="">Select an option</option>
                )}
              </select>
            ) : null}

            {inputKind !== "bool" && inputKind !== "select" ? (
              <textarea
                value={textValue}
                onChange={(event) => setTextValue(event.target.value)}
                placeholder="Type your answer…"
                rows={4}
                style={{ ...inputStyle, resize: "vertical", width: "100%" }}
              />
            ) : null}
          </div>
        )}

        {operatorProfile.error ? (
          <div style={{ border: "1px solid rgba(255,0,0,0.3)", color: "#ffb4b4", padding: 8, fontSize: 12 }}>
            {operatorProfile.error}
          </div>
        ) : null}

        <div style={{ display: "flex", justifyContent: "space-between", gap: 8, flexWrap: "wrap" }}>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            <button
              type="button"
              onClick={() => void skipQuestion("skipped_from_onboarding_panel")}
              style={secondaryButtonStyle}
              disabled={!question || operatorProfile.loading}
            >
              Skip
            </button>
            <button
              type="button"
              onClick={() => void deferQuestion(Date.now() + 24 * 60 * 60 * 1000)}
              style={secondaryButtonStyle}
              disabled={!question || operatorProfile.loading}
            >
              Defer 24h
            </button>
          </div>

          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            <button
              type="button"
              onClick={() => void fetchNextQuestion()}
              style={secondaryButtonStyle}
              disabled={operatorProfile.loading || !operatorProfile.sessionId}
            >
              Refresh
            </button>
            <button
              type="button"
              onClick={() => void submitValue()}
              style={primaryButtonStyle}
              disabled={!canSubmit || operatorProfile.loading}
            >
              Submit
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

const inputStyle: CSSProperties = {
  background: "rgba(255,255,255,0.04)",
  border: "1px solid rgba(255,255,255,0.15)",
  color: "var(--text-primary)",
  fontFamily: "inherit",
  fontSize: 13,
  padding: "8px 10px",
};

const primaryButtonStyle: CSSProperties = {
  border: "1px solid rgba(97, 197, 255, 0.42)",
  background: "rgba(97, 197, 255, 0.18)",
  color: "var(--text-primary)",
  padding: "8px 12px",
  fontSize: 12,
  cursor: "pointer",
};

const secondaryButtonStyle: CSSProperties = {
  border: "1px solid rgba(255,255,255,0.2)",
  background: "rgba(255,255,255,0.05)",
  color: "var(--text-primary)",
  padding: "8px 12px",
  fontSize: 12,
  cursor: "pointer",
};

const selectedButtonStyle: CSSProperties = {
  ...primaryButtonStyle,
  fontWeight: 700,
};
