import { invoke } from "@tauri-apps/api/core";

// Mirrors the serde output of the `mh_core` and `mh_store` types.

export type EditorialStatus =
  | "submitted"
  | "with_editor"
  | "under_review"
  | "revision_required"
  | "revised"
  | "accepted"
  | "rejected"
  | "withdrawn";

export type ReviewState = "proposed" | "confirmed" | "edited" | "ignored" | "unrelated";

export type MatchKind = "exact_id" | "id_only" | "title" | "ambiguous" | "none" | "manual";

export interface CalendarDate {
  year: number;
  month: number;
  day: number;
}

export interface Field<T> {
  value: T;
  original: string;
  /** 0..1 */
  confidence: number;
}

export interface SourceRef {
  provider: "gmail" | "microsoft_graph" | "groupwise_imap" | "mock";
  account_alias: string;
  message_id: string;
  folder: string;
  /** Unix seconds, UTC */
  received_at: number;
}

export interface ProposedEvent {
  source: SourceRef;
  journal: Field<string> | null;
  manuscript_id: Field<string> | null;
  title: Field<string> | null;
  status: Field<EditorialStatus> | null;
  deadline: Field<CalendarDate> | null;
  requested_action: Field<string> | null;
  parser_version: string;
  uncertainty: string[];
  review: ReviewState;
}

export interface ManuscriptRef {
  id: number;
  journal: string;
  external_id: string | null;
  title: string | null;
}

export interface QueueItem {
  event_id: number;
  proposal: ProposedEvent;
  match_kind: MatchKind;
  match_rationale: string;
  suggested_manuscript: ManuscriptRef | null;
}

/** Replacement values; `null` clears a field. */
export interface EventEdits {
  status: EditorialStatus | null;
  deadline: CalendarDate | null;
  requested_action: string | null;
}

export type ReviewDecision =
  | { kind: "confirm"; manuscript_id?: number | null; edits?: EventEdits | null }
  | { kind: "ignore" }
  | { kind: "unrelated" };

export interface ManuscriptSummary {
  id: number;
  journal: string;
  external_id: string | null;
  title: string | null;
  status: EditorialStatus | null;
  status_since: number | null;
  deadline: CalendarDate | null;
  requested_action: string | null;
  pending_reviews: number;
}

export interface TimelineEntry {
  event_id: number;
  occurred_at: number;
  review_state: ReviewState;
  status: EditorialStatus | null;
  original_phrase: string | null;
  deadline: CalendarDate | null;
  requested_action: string | null;
  source: SourceRef | null;
}

export interface ScanSummary {
  messages: number;
  proposed: number;
  already_seen: number;
  not_journal_mail: number;
}

export interface AppInfo {
  version: string;
  parser_version: string;
  /** Why the encrypted database could not be opened, if it failed. */
  storage_error: string | null;
}

/** True inside the desktop shell; false when the UI is opened in a plain browser. */
export function inDesktopShell(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export type Api = typeof desktopApi;

const desktopApi = {
  appInfo: () => invoke<AppInfo>("app_info"),
  scanDemoMailbox: () => invoke<ScanSummary>("scan_demo_mailbox"),
  reviewQueue: () => invoke<QueueItem[]>("review_queue"),
  reviewEvent: (eventId: number, decision: ReviewDecision) =>
    invoke<void>("review_event", { eventId, decision }),
  manuscripts: () => invoke<ManuscriptSummary[]>("manuscripts"),
  timeline: (manuscriptId: number) => invoke<TimelineEntry[]>("timeline", { manuscriptId }),
};

/** In a dev browser (`npm run dev`) the UI runs on an in-memory mock; production builds never include it. */
export const usingMock = !inDesktopShell() && import.meta.env.DEV;

export const api: Api = usingMock ? (await import("./mock")).mockApi : desktopApi;
