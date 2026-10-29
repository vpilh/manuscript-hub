import type { CalendarDate, EditorialStatus } from "./api";

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

export function statusLabel(status: EditorialStatus): string {
  return STATUS_LABELS[status];
}

const pad = (n: number) => String(n).padStart(2, "0");

export function isoDate(date: CalendarDate): string {
  return `${date.year}-${pad(date.month)}-${pad(date.day)}`;
}

/** Whole days from `today` until `date`; negative when overdue. */
export function daysUntil(date: CalendarDate, today: Date): number {
  const target = Date.UTC(date.year, date.month - 1, date.day);
  const start = Date.UTC(today.getFullYear(), today.getMonth(), today.getDate());
  return Math.round((target - start) / 86_400_000);
}

export function confidenceLabel(confidence: number): "high" | "medium" | "low" {
  if (confidence >= 0.75) return "high";
  if (confidence >= 0.5) return "medium";
  return "low";
}

export const ALL_STATUSES = Object.keys(STATUS_LABELS) as EditorialStatus[];

/** `YYYY-MM-DD` from an `<input type="date">`, or null if empty or invalid. */
export function parseIsoDate(value: string): CalendarDate | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return null;
  const [year, month, day] = match.slice(1).map(Number);
  const check = new Date(Date.UTC(year, month - 1, day));
  if (check.getUTCMonth() !== month - 1 || check.getUTCDate() !== day) return null;
  return { year, month, day };
}

export function formatTimestamp(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}
