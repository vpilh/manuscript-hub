import type { CalendarDate, EditorialStatus, ScanSummary } from "./api";

const STATUS_LABELS: Record<EditorialStatus, string> = {
  submitted: "Submitted",
  with_editor: "With editor",
  under_review: "Under review",
  revision_required: "Revision required",
  revised: "Revised",
  accepted: "Accepted",
  rejected: "Rejected",
  withdrawn: "Withdrawn",
};

export const ALL_STATUSES = Object.keys(STATUS_LABELS) as EditorialStatus[];

export function statusLabel(status: EditorialStatus): string {
  return STATUS_LABELS[status];
}

const pad = (n: number) => String(n).padStart(2, "0");

export function isoDate(date: CalendarDate): string {
  return `${date.year}-${pad(date.month)}-${pad(date.day)}`;
}

// The interface copy is English, so dates use one English format everywhere.
const LONG_DATE = new Intl.DateTimeFormat("en-GB", {
  day: "numeric",
  month: "long",
  year: "numeric",
  timeZone: "UTC",
});

/** `12 November 2026` */
export function longDate(date: CalendarDate): string {
  return LONG_DATE.format(new Date(Date.UTC(date.year, date.month - 1, date.day)));
}

/** A received-at timestamp as a calendar date in the user's timezone. */
export function formatTimestamp(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleDateString("en-GB", {
    day: "numeric",
    month: "short",
    year: "numeric",
  });
}

/** Whole days from `today` until `date`; negative when overdue. */
export function daysUntil(date: CalendarDate, today: Date): number {
  const target = Date.UTC(date.year, date.month - 1, date.day);
  const start = Date.UTC(today.getFullYear(), today.getMonth(), today.getDate());
  return Math.round((target - start) / 86_400_000);
}

/** `in 14 days`, `tomorrow`, `today`, `3 days overdue` */
export function relativeDue(days: number): string {
  if (days === 0) return "today";
  if (days === 1) return "tomorrow";
  if (days > 1) return `in ${days} days`;
  return days === -1 ? "1 day overdue" : `${-days} days overdue`;
}

/** Deadlines within a week, or past, are drawn in proof red. */
export function isUrgent(days: number): boolean {
  return days <= 7;
}

export function confidenceLabel(confidence: number): "high" | "medium" | "low" {
  if (confidence >= 0.75) return "high";
  if (confidence >= 0.5) return "medium";
  return "low";
}

const CONFIDENCE_WORDS = { high: "clear", medium: "probable", low: "uncertain" } as const;

/** Written-out confidence, so it never depends on color alone. */
export function confidenceWord(confidence: number): string {
  return CONFIDENCE_WORDS[confidenceLabel(confidence)];
}

/** `YYYY-MM-DD` from an `<input type="date">`, or null if empty or invalid. */
export function parseIsoDate(value: string): CalendarDate | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return null;
  const [year, month, day] = match.slice(1).map(Number);
  const check = new Date(Date.UTC(year, month - 1, day));
  if (check.getUTCMonth() !== month - 1 || check.getUTCDate() !== day) return null;
  return { year, month, day };
}

/** "just now", "5 minutes ago", "3 hours ago", then a date. */
export function checkedAgo(unixSeconds: number, nowSeconds: number): string {
  const minutes = Math.floor((nowSeconds - unixSeconds) / 60);
  if (minutes < 1) return "just now";
  if (minutes < 60) return minutes === 1 ? "1 minute ago" : `${minutes} minutes ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return hours === 1 ? "1 hour ago" : `${hours} hours ago`;
  return `on ${formatTimestamp(unixSeconds)}`;
}

/** The outcome of a mailbox check, in one sentence. */
export function scanSentence(scan: ScanSummary): string {
  if (scan.messages === 0) return "No new mail since the last check.";
  const read = scan.messages === 1 ? "Read 1 message" : `Read ${scan.messages} messages`;
  const fresh = scan.proposed === 1 ? "1 new to review" : `${scan.proposed} new to review`;
  return `${read}: ${fresh}, ${scan.already_seen} seen before, ${scan.not_journal_mail} not journal mail.`;
}

const FINISHED: EditorialStatus[] = ["accepted", "rejected", "withdrawn"];
const WITH_THE_JOURNAL: EditorialStatus[] = ["submitted", "with_editor", "under_review", "revised"];

/** Accepted, rejected or withdrawn: nothing more is expected. */
export function isFinished(status: EditorialStatus | null): boolean {
  return status !== null && FINISHED.includes(status);
}

/** How long the journal has had the manuscript, for statuses where the author waits. */
export function waitingFor(
  status: EditorialStatus | null,
  sinceSeconds: number | null,
  nowSeconds: number,
): string | null {
  if (!status || sinceSeconds === null || !WITH_THE_JOURNAL.includes(status)) return null;
  const days = Math.floor((nowSeconds - sinceSeconds) / 86_400);
  if (days < 1) return null;
  if (days < 14) return days === 1 ? "waiting 1 day" : `waiting ${days} days`;
  if (days < 60) return `waiting ${Math.floor(days / 7)} weeks`;
  return `waiting ${Math.floor(days / 30)} months`;
}
