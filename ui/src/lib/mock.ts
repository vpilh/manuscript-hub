// Dev-only stand-in for the Rust backend, so the UI can be designed and
// previewed in a browser with `npm run dev`. Data mirrors fixtures/synthetic/.
// Production builds never include this module (see `usingMock` in api.ts).

import type {
  Api,
  CalendarDate,
  EditorialStatus,
  MailAccount,
  ManuscriptSummary,
  ProposedEvent,
  QueueItem,
  ReviewDecision,
  TimelineEntry,
} from "./api";

const DAY = 86_400;
const now = Math.floor(Date.now() / 1000);

function dateIn(days: number): CalendarDate {
  const d = new Date(Date.now() + days * DAY * 1000);
  return { year: d.getUTCFullYear(), month: d.getUTCMonth() + 1, day: d.getUTCDate() };
}

const field = <T,>(value: T, original = String(value), confidence = 0.8) => ({
  value,
  original,
  confidence,
});

function proposal(
  id: string,
  receivedDaysAgo: number,
  journal: string | null,
  manuscriptId: string | null,
  status: [EditorialStatus, string, number] | null,
  deadline: [number, number] | null,
  uncertainty: string[] = [],
  action: string | null = null,
): ProposedEvent {
  return {
    source: {
      provider: "mock",
      account_alias: "Demo mailbox",
      message_id: `<${id}@example.org>`,
      folder: "INBOX",
      received_at: now - receivedDaysAgo * DAY,
    },
    journal: journal ? field(journal, journal, 0.5) : null,
    manuscript_id: manuscriptId ? field(manuscriptId) : null,
    title: null,
    status: status ? field(status[0], status[1], status[2]) : null,
    deadline: deadline ? field(dateIn(deadline[0]), "deadline", deadline[1]) : null,
    requested_action: action ? field(action) : null,
    parser_version: "seed-0.1",
    uncertainty,
    review: "proposed",
  };
}

let manuscripts: ManuscriptSummary[] = [
  {
    id: 1,
    journal: "Journal of Synthetic Studies",
    external_id: "JSS-2026-0142",
    title: null,
    status: "under_review",
    status_since: now - 40 * DAY,
    deadline: null,
    requested_action: null,
    pending_reviews: 1,
  },
  {
    id: 2,
    journal: "Zeitschrift für Synthetische Studien",
    external_id: "ZSS-2026-077",
    title: null,
    status: "accepted",
    status_since: now - 3 * DAY,
    deadline: dateIn(5),
    requested_action: "Return the signed licence agreement",
    pending_reviews: 0,
  },
];

let queue: QueueItem[] = [
  {
    event_id: 11,
    proposal: proposal(
      "jss-0142-decision",
      0,
      "Journal of Synthetic Studies",
      "JSS-2026-0142",
      ["revision_required", "major revision", 0.7],
      [14, 0.8],
      [],
      "Submit revised manuscript",
    ),
    match_kind: "exact_id",
    match_rationale: "Same manuscript ID JSS-2026-0142 at Journal of Synthetic Studies.",
    suggested_manuscript: {
      id: 1,
      journal: "Journal of Synthetic Studies",
      external_id: "JSS-2026-0142",
      title: null,
    },
  },
  {
    event_id: 12,
    proposal: proposal(
      "jss-status-digest",
      2,
      "Journal of Synthetic Studies",
      "JSS-2026-0142",
      ["under_review", "under review", 0.7],
      null,
      ["Message mentions 2 possible manuscript IDs."],
    ),
    match_kind: "exact_id",
    match_rationale: "Same manuscript ID JSS-2026-0142 at Journal of Synthetic Studies.",
    suggested_manuscript: {
      id: 1,
      journal: "Journal of Synthetic Studies",
      external_id: "JSS-2026-0142",
      title: null,
    },
  },
  {
    event_id: 13,
    proposal: proposal(
      "jss-0177-received",
      9,
      "Journal of Synthetic Studies",
      "JSS-2026-0177",
      ["submitted", "Thank you for submitting", 0.7],
      null,
    ),
    match_kind: "none",
    match_rationale: "No tracked manuscript has ID JSS-2026-0177; confirming adds it.",
    suggested_manuscript: null,
  },
  {
    event_id: 14,
    proposal: proposal("jss-0142-reply", 0, "Alex Example", "JSS-2026-0142", null, null, [
      "No editorial status recognized.",
    ]),
    match_kind: "id_only",
    match_rationale:
      "Same manuscript ID JSS-2026-0142 as the manuscript at Journal of Synthetic Studies; the sender's journal is unknown.",
    suggested_manuscript: {
      id: 1,
      journal: "Journal of Synthetic Studies",
      external_id: "JSS-2026-0142",
      title: null,
    },
  },
];

const timelines: Record<number, TimelineEntry[]> = {
  1: [
    {
      event_id: 2,
      occurred_at: now - 40 * DAY,
      review_state: "confirmed",
      status: "under_review",
      original_phrase: "sent out for review",
      deadline: null,
      requested_action: null,
      source: {
        provider: "mock",
        account_alias: "Demo mailbox",
        message_id: "<jss-0142-review@example.org>",
        folder: "INBOX",
        received_at: now - 40 * DAY,
      },
    },
    {
      event_id: 1,
      occurred_at: now - 52 * DAY,
      review_state: "confirmed",
      status: "submitted",
      original_phrase: "successfully submitted",
      deadline: null,
      requested_action: null,
      source: {
        provider: "mock",
        account_alias: "Demo mailbox",
        message_id: "<jss-0142-received@example.org>",
        folder: "INBOX",
        received_at: now - 52 * DAY,
      },
    },
  ],
  2: [
    {
      event_id: 3,
      occurred_at: now - 3 * DAY,
      review_state: "edited",
      status: "accepted",
      original_phrase: "zur Veröffentlichung angenommen",
      deadline: dateIn(5),
      requested_action: "Return the signed licence agreement",
      source: {
        provider: "mock",
        account_alias: "Demo mailbox",
        message_id: "<zss-2026-77-entscheidung@example.org>",
        folder: "INBOX",
        received_at: now - 3 * DAY,
      },
    },
  ],
};

let accounts: MailAccount[] = [];

const wait = <T,>(value: T) => new Promise<T>((resolve) => setTimeout(() => resolve(value), 120));

function decide(eventId: number, decision: ReviewDecision) {
  const item = queue.find((q) => q.event_id === eventId);
  if (!item) throw new Error(`event ${eventId} has already been reviewed`);
  queue = queue.filter((q) => q.event_id !== eventId);
  if (decision.kind !== "confirm") return;

  const p = item.proposal;
  let target = manuscripts.find(
    (m) => m.id === (decision.manuscript_id ?? item.suggested_manuscript?.id),
  );
  if (!target) {
    target = {
      id: Math.max(0, ...manuscripts.map((m) => m.id)) + 1,
      journal: p.journal?.value ?? "Unknown journal",
      external_id: p.manuscript_id?.value ?? null,
      title: null,
      status: null,
      status_since: null,
      deadline: null,
      requested_action: null,
      pending_reviews: 0,
    };
    manuscripts = [target, ...manuscripts];
  }
  const status = decision.edits ? decision.edits.status : (p.status?.value ?? null);
  const deadline = decision.edits ? decision.edits.deadline : (p.deadline?.value ?? null);
  const action = decision.edits
    ? decision.edits.requested_action
    : (p.requested_action?.value ?? null);
  if (status || deadline) {
    target.status = status ?? target.status;
    target.status_since = status ? p.source.received_at : target.status_since;
    target.deadline = deadline;
    target.requested_action = action;
  }
  (timelines[target.id] ??= []).unshift({
    event_id: eventId,
    occurred_at: p.source.received_at,
    review_state: decision.edits ? "edited" : "confirmed",
    status,
    original_phrase: p.status?.original ?? null,
    deadline,
    requested_action: action,
    source: p.source,
  });
}

function withPendingCounts(): ManuscriptSummary[] {
  return manuscripts.map((m) => ({
    ...m,
    pending_reviews: queue.filter((q) => q.suggested_manuscript?.id === m.id).length,
  }));
}

export const mockApi: Api = {
  appInfo: () => wait({ version: "0.1.0", parser_version: "seed-0.3", storage_error: null }),
  scanDemoMailbox: () =>
    wait({ messages: 6, proposed: 0, already_seen: 5, not_journal_mail: 1 }),
  reviewQueue: () => wait(structuredClone(queue)),
  reviewEvent: async (eventId, decision) => {
    decide(eventId, decision);
    await wait(undefined);
  },
  manuscripts: () => wait(structuredClone(withPendingCounts())),
  timeline: (manuscriptId) => wait(structuredClone(timelines[manuscriptId] ?? [])),
  mergeManuscripts: async (fromId, intoId) => {
    const into = manuscripts.find((m) => m.id === intoId);
    if (!into || fromId === intoId) throw new Error("manuscript not found");
    timelines[intoId] = [...(timelines[intoId] ?? []), ...(timelines[fromId] ?? [])].sort(
      (a, b) => b.occurred_at - a.occurred_at,
    );
    delete timelines[fromId];
    queue = queue.map((q) =>
      q.suggested_manuscript?.id === fromId
        ? { ...q, suggested_manuscript: { ...q.suggested_manuscript, id: intoId } }
        : q,
    );
    const newest = timelines[intoId].find((e) => e.status);
    if (newest) {
      into.status = newest.status;
      into.status_since = newest.occurred_at;
    }
    manuscripts = manuscripts.filter((m) => m.id !== fromId);
    await wait(undefined);
  },
  mailAccounts: () => wait(structuredClone(accounts)),
  // The password "wrong" simulates a rejected login.
  connectMailAccount: async ({ provider, alias, settings, password }) => {
    await wait(undefined);
    if (password === "wrong") {
      throw new Error(
        "The server rejected the user name or password. For Gmail, use an app password, not your Google password.",
      );
    }
    const account: MailAccount = {
      id: Math.max(0, ...accounts.map((a) => a.id)) + 1,
      provider,
      alias,
      settings: {
        ...settings,
        folders: settings.folders.length
          ? settings.folders
          : [provider === "gmail" ? "[Gmail]/All Mail" : "INBOX"],
      },
      last_sync_at: null,
      last_error: null,
    };
    accounts = [...accounts, account];
    return structuredClone(account);
  },
  disconnectMailAccount: async (accountId) => {
    accounts = accounts.filter((a) => a.id !== accountId);
    await wait(undefined);
  },
  syncMailAccount: async (accountId) => {
    await new Promise((resolve) => setTimeout(resolve, 900));
    accounts = accounts.map((a) =>
      a.id === accountId ? { ...a, last_sync_at: Math.floor(Date.now() / 1000) } : a,
    );
    return { messages: 214, proposed: 0, already_seen: 5, not_journal_mail: 209 };
  },
};
