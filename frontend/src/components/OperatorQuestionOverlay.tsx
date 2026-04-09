import { useEffect, useState, type CSSProperties } from "react";
import { getBridge } from "@/lib/bridge";

type PendingQuestion = {
  question_id: string;
  content: string;
  options: string[];
};

export function OperatorQuestionOverlay() {
  const [question, setQuestion] = useState<PendingQuestion | null>(null);
  const [busyAnswer, setBusyAnswer] = useState<string | null>(null);

  useEffect(() => {
    const bridge = getBridge();
    if (!bridge?.onAgentEvent) return;
    return bridge.onAgentEvent((event: any) => {
      if (event?.type === "operator_question") {
        const options = Array.isArray(event.options)
          ? event.options.filter((value: unknown): value is string => typeof value === "string" && value.trim().length > 0)
          : [];
        const questionId = typeof event.question_id === "string" ? event.question_id : "";
        const content = typeof event.content === "string" ? event.content : "";
        if (!questionId || !content || options.length === 0) return;
        setQuestion({ question_id: questionId, content, options });
        setBusyAnswer(null);
      }
      if (event?.type === "operator_question_resolved") {
        const resolvedId = typeof event.question_id === "string" ? event.question_id : "";
        setQuestion((current) => (current && current.question_id === resolvedId ? null : current));
        setBusyAnswer(null);
      }
    });
  }, []);

  if (!question) {
    return null;
  }

  const submit = async (answer: string) => {
    if (!question || busyAnswer) return;
    setBusyAnswer(answer);
    const bridge = getBridge();
    try {
      await bridge?.agentAnswerQuestion?.(question.question_id, answer);
    } finally {
      setBusyAnswer(null);
    }
  };

  return (
    <div style={backdropStyle}>
      <div style={panelStyle}>
        <div style={eyebrowStyle}>Question</div>
        <div style={contentStyle}>{question.content}</div>
        <div style={buttonRowStyle}>
          {question.options.map((option) => {
            const active = busyAnswer === option;
            return (
              <button
                key={option}
                type="button"
                onClick={() => void submit(option)}
                disabled={Boolean(busyAnswer)}
                style={{
                  ...buttonStyle,
                  ...(active ? activeButtonStyle : null),
                }}
              >
                {option}
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}

const backdropStyle: CSSProperties = {
  position: "fixed",
  inset: 0,
  zIndex: 4400,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  padding: 24,
  background: "rgba(6, 10, 18, 0.72)",
};

const panelStyle: CSSProperties = {
  width: "min(720px, 96vw)",
  display: "grid",
  gap: 18,
  padding: 22,
  border: "1px solid rgba(255,255,255,0.16)",
  background: "linear-gradient(180deg, rgba(14, 21, 31, 0.98), rgba(9, 15, 23, 0.98))",
  boxShadow: "0 24px 80px rgba(0,0,0,0.42)",
};

const eyebrowStyle: CSSProperties = {
  fontSize: 12,
  letterSpacing: "0.08em",
  textTransform: "uppercase",
  color: "var(--text-secondary)",
};

const contentStyle: CSSProperties = {
  whiteSpace: "pre-wrap",
  lineHeight: 1.55,
  fontSize: 16,
  color: "var(--text-primary)",
};

const buttonRowStyle: CSSProperties = {
  display: "flex",
  gap: 10,
  flexWrap: "wrap",
};

const buttonStyle: CSSProperties = {
  minWidth: 56,
  padding: "10px 16px",
  border: "1px solid rgba(255,255,255,0.18)",
  background: "rgba(255,255,255,0.04)",
  color: "var(--text-primary)",
  fontWeight: 700,
  cursor: "pointer",
};

const activeButtonStyle: CSSProperties = {
  borderColor: "var(--accent)",
  background: "color-mix(in srgb, var(--accent) 18%, transparent)",
};