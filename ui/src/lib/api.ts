import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

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
  /** Why the message was treated as journal mail; empty on older proposals. */
  reason: string;
  /** The sender's domain, for "never show mail from …"; null on older proposals. */
  sender_domain: string | null;
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

export type ManualTarget =
  | { kind: "existing"; manuscript_id: number }
  | { kind: "new"; journal: string; external_id: string | null; title: string | null };

export interface ManualUpdate {
  occurred_on: CalendarDate;
  status: EditorialStatus | null;
  deadline: CalendarDate | null;
  requested_action: string | null;
}

/** What happened to a pasted message. */
export type PasteOutcome = "proposed" | "duplicate" | "nothing_found";

/** "journal": always journal mail; "ignore": never. */
export type SenderRuleKind = "journal" | "ignore";

export interface SenderRule {
  domain: string;
  kind: SenderRuleKind;
}

export type Security = "tls" | "start_tls";

/** Connection settings without the password, which stays in the OS keychain. */
export interface ImapSettings {
  host: string;
  port: number;
  security: Security;
  username: string;
  /** Empty when connecting: Gmail's All Mail (or the inbox) is chosen automatically. */
  folders: string[];
  backfill_days: number;
  /** Minutes between automatic checks; 0 means only on request. */
  check_every_minutes: number;
}

export type MailProvider = "gmail" | "groupwise_imap";

export interface MailAccount {
  id: number;
  provider: MailProvider;
  alias: string;
  settings: ImapSettings;
  /** Unix seconds of the last successful check. */
  last_sync_at: number | null;
  last_error: string | null;
}

/** Sent by the backend when a mailbox check starts (`running`) and ends. */
export interface CheckEvent {
  account_id: number;
  running: boolean;
  summary: ScanSummary | null;
  error: string | null;
}

export interface NewMailAccount {
  provider: MailProvider;
  alias: string;
  settings: ImapSettings;
  password: string;
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
  mergeManuscripts: (fromId: number, intoId: number) =>
    invoke<void>("merge_manuscripts", { fromId, intoId }),
  amendEvent: (eventId: number, edits: EventEdits) =>
    invoke<void>("amend_event", { eventId, edits }),
  removeEvent: (eventId: number) => invoke<void>("remove_event", { eventId }),
  mailAccounts: () => invoke<MailAccount[]>("mail_accounts"),
  connectMailAccount: (account: NewMailAccount) =>
    invoke<MailAccount>("connect_mail_account", { account }),
  disconnectMailAccount: (accountId: number) =>
    invoke<void>("disconnect_mail_account", { accountId }),
  syncMailAccount: (accountId: number) => invoke<ScanSummary>("sync_mail_account", { accountId }),
  /** Resolves to the manuscript the update was added to. */
  addManualUpdate: (target: ManualTarget, update: ManualUpdate) =>
    invoke<number>("add_manual_update", { target, update }),
  importPastedMail: (text: string) => invoke<PasteOutcome>("import_pasted_mail", { text }),
  senderRules: () => invoke<SenderRule[]>("sender_rules"),
  /** Resolves to how many waiting proposals an "ignore" rule set aside. */
  setSenderRule: (domain: string, kind: SenderRuleKind) =>
    invoke<number>("set_sender_rule", { domain, kind }),
  removeSenderRule: (domain: string) => invoke<void>("remove_sender_rule", { domain }),
  setCheckInterval: (accountId: number, minutes: number) =>
    invoke<void>("set_check_interval", { accountId, minutes }),
  checksPaused: () => invoke<boolean>("checks_paused"),
  setChecksPaused: (paused: boolean) => invoke<void>("set_checks_paused", { paused }),
  /** Follows pausing from the tray menu too; resolves to an unsubscribe function. */
  onChecksPaused: (handler: (paused: boolean) => void): Promise<() => void> =>
    listen<boolean>("checks-paused", (e) => handler(e.payload)),
  /** Calls `handler` for every check, including scheduled ones; resolves to an unsubscribe function. */
  onMailCheck: (handler: (event: CheckEvent) => void): Promise<() => void> =>
    listen<CheckEvent>("mail-check", (e) => handler(e.payload)),
};

/** In a dev browser (`npm run dev`) the UI runs on an in-memory mock; production builds never include it. */
export const usingMock = import.meta.env.DEV && !inDesktopShell();

export const api: Api = usingMock ? (await import("./mock")).mockApi : desktopApi;
