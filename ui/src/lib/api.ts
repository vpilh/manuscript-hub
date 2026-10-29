import { invoke } from "@tauri-apps/api/core";

// Mirrors the serde output of `mh_core` types.

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

export interface AppInfo {
  version: string;
  parser_version: string;
}

/** True inside the desktop shell; false when the UI is opened in a plain browser. */
export function inDesktopShell(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  scanDemoMailbox: () => invoke<ProposedEvent[]>("scan_demo_mailbox"),
};
