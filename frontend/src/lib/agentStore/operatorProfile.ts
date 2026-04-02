export interface OperatorProfileQuestion {
  session_id: string;
  question_id: string;
  field_key: string;
  prompt: string;
  input_kind: string;
  optional: boolean;
}

export interface OperatorProfileProgress {
  session_id: string;
  answered: number;
  remaining: number;
  completion_ratio: number;
}

export interface OperatorProfileConsent {
  consent_key: string;
  granted: boolean;
  updated_at: number;
}

export interface OperatorProfileCheckin {
  id: string;
  kind: string;
  status: string;
  scheduled_at: number | null;
  shown_at: number | null;
  response_json: string | null;
}

export interface OperatorProfileSummary {
  field_count: number;
  fields: Record<string, {
    value: unknown;
    confidence: number;
    source: string;
    updated_at: number;
  }>;
  consents: OperatorProfileConsent[];
  checkins: OperatorProfileCheckin[];
}

export interface OperatorProfileState {
  panelOpen: boolean;
  loading: boolean;
  error: string | null;
  sessionId: string | null;
  sessionKind: string | null;
  question: OperatorProfileQuestion | null;
  progress: OperatorProfileProgress | null;
  summary: OperatorProfileSummary | null;
  lastCompletedAt: number | null;
}

export type OperatorProfileSessionStarted = {
  session_id: string;
  kind?: string;
};

export type OperatorProfileSessionCompleted = {
  session_id: string;
  updated_fields?: string[];
};

export const DEFAULT_OPERATOR_PROFILE_STATE: OperatorProfileState = {
  panelOpen: false,
  loading: false,
  error: null,
  sessionId: null,
  sessionKind: null,
  question: null,
  progress: null,
  summary: null,
  lastCompletedAt: null,
};

const OPERATOR_PROFILE_REQUIRED_FIELDS = ["name", "role", "primary_language"] as const;

export function isOperatorProfileError(value: unknown): value is { error: string } {
  return Boolean(
    value
    && typeof value === "object"
    && typeof (value as { error?: unknown }).error === "string",
  );
}

export function isOperatorProfileSessionStarted(
  value: unknown,
): value is OperatorProfileSessionStarted {
  return Boolean(
    value
    && typeof value === "object"
    && typeof (value as OperatorProfileSessionStarted).session_id === "string",
  );
}

export function isOperatorProfileQuestion(value: unknown): value is OperatorProfileQuestion {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const question = value as Partial<OperatorProfileQuestion>;
  return (
    typeof question.session_id === "string"
    && typeof question.question_id === "string"
    && typeof question.field_key === "string"
    && typeof question.prompt === "string"
    && typeof question.input_kind === "string"
    && typeof question.optional === "boolean"
  );
}

export function isOperatorProfileProgress(value: unknown): value is OperatorProfileProgress {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const progress = value as Partial<OperatorProfileProgress>;
  return (
    typeof progress.session_id === "string"
    && typeof progress.answered === "number"
    && typeof progress.remaining === "number"
    && typeof progress.completion_ratio === "number"
  );
}

export function isOperatorProfileSessionCompleted(
  value: unknown,
): value is OperatorProfileSessionCompleted {
  return Boolean(
    value
    && typeof value === "object"
    && typeof (value as OperatorProfileSessionCompleted).session_id === "string",
  );
}

export function parseOperatorProfileSummary(value: unknown): OperatorProfileSummary | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  const record = value as Record<string, unknown>;
  const field_count = typeof record.field_count === "number" ? record.field_count : 0;
  const rawFields = record.fields;
  const fields = (rawFields && typeof rawFields === "object" && !Array.isArray(rawFields))
    ? rawFields as Record<string, { value: unknown; confidence?: number; source?: string; updated_at?: number }>
    : {};
  const normalizedFields: OperatorProfileSummary["fields"] = {};
  for (const [key, entry] of Object.entries(fields)) {
    if (!entry || typeof entry !== "object" || Array.isArray(entry)) {
      continue;
    }
    normalizedFields[key] = {
      value: entry.value,
      confidence: typeof entry.confidence === "number" ? entry.confidence : 0,
      source: typeof entry.source === "string" ? entry.source : "unknown",
      updated_at: typeof entry.updated_at === "number" ? entry.updated_at : 0,
    };
  }
  const consents = Array.isArray(record.consents)
    ? record.consents
      .filter((entry) => entry && typeof entry === "object")
      .map((entry) => {
        const consent = entry as Record<string, unknown>;
        return {
          consent_key: typeof consent.consent_key === "string" ? consent.consent_key : "",
          granted: consent.granted === true,
          updated_at: typeof consent.updated_at === "number" ? consent.updated_at : 0,
        } satisfies OperatorProfileConsent;
      })
      .filter((entry) => entry.consent_key.length > 0)
    : [];
  const checkins = Array.isArray(record.checkins)
    ? record.checkins
      .filter((entry) => entry && typeof entry === "object")
      .map((entry) => {
        const checkin = entry as Record<string, unknown>;
        return {
          id: typeof checkin.id === "string" ? checkin.id : "",
          kind: typeof checkin.kind === "string" ? checkin.kind : "unknown",
          status: typeof checkin.status === "string" ? checkin.status : "unknown",
          scheduled_at: typeof checkin.scheduled_at === "number" ? checkin.scheduled_at : null,
          shown_at: typeof checkin.shown_at === "number" ? checkin.shown_at : null,
          response_json: typeof checkin.response_json === "string" ? checkin.response_json : null,
        } satisfies OperatorProfileCheckin;
      })
      .filter((entry) => entry.id.length > 0)
    : [];

  return {
    field_count,
    fields: normalizedFields,
    consents,
    checkins,
  };
}

export function hasRequiredOperatorProfileFields(summary: OperatorProfileSummary | null): boolean {
  if (!summary) {
    return false;
  }
  return OPERATOR_PROFILE_REQUIRED_FIELDS.every((fieldKey) =>
    Object.prototype.hasOwnProperty.call(summary.fields, fieldKey),
  );
}
