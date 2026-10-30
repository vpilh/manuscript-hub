import { ChevronDown } from "lucide-react";
import { useEffect, useId, useState } from "react";

import { api, type EventEdits, type ManuscriptSummary, type TimelineEntry } from "../lib/api";
import {
  daysUntil,
  formatTimestamp,
  isUrgent,
  longDate,
  relativeDue,
  statusLabel,
} from "../lib/format";
import { EventForm } from "./EventForm";
import { StatusMark } from "./StatusMark";

interface Props {
  manuscripts: ManuscriptSummary[];
  initialOpenId: number | null;
  onOpenReview: () => void;
  onMerge: (fromId: number, intoId: number) => Promise<void>;
  onAmend: (eventId: number, edits: EventEdits) => Promise<void>;
  onRemove: (eventId: number) => Promise<void>;
}

export function Manuscripts(props: Props) {
  const { manuscripts, initialOpenId, onOpenReview, onMerge, onAmend, onRemove } = props;
  const [openId, setOpenId] = useState<number | null>(initialOpenId);

  if (manuscripts.length === 0) {
    return (
      <div className="empty">
        <p className="empty-title">No manuscripts tracked yet.</p>
        <p>
          Add a message from the review list and its manuscript starts here, with every update in
          order.
        </p>
        <button className="secondary" onClick={onOpenReview}>
          Go to review
        </button>
      </div>
    );
  }

  return (
    <ul className="ledger" aria-label="Tracked manuscripts">
      {manuscripts.map((m) => (
        <LedgerRow
          key={m.id}
          manuscript={m}
          open={openId === m.id}
          onToggle={() => setOpenId(openId === m.id ? null : m.id)}
          others={manuscripts.filter((other) => other.id !== m.id)}
          onAmend={onAmend}
          onRemove={onRemove}
          onMerge={async (intoId) => {
            await onMerge(m.id, intoId);
            setOpenId(intoId);
          }}
        />
      ))}
    </ul>
  );
}

function LedgerRow(props: {
  manuscript: ManuscriptSummary;
  open: boolean;
  onToggle: () => void;
  others: ManuscriptSummary[];
  onMerge: (intoId: number) => Promise<void>;
  onAmend: (eventId: number, edits: EventEdits) => Promise<void>;
  onRemove: (eventId: number) => Promise<void>;
}) {
  const { manuscript: m, open, onToggle } = props;
  const panelId = useId();
  const days = m.deadline ? daysUntil(m.deadline, new Date()) : null;

  return (
    <li className={open ? "ledger-row open" : "ledger-row"}>
      <button
        className="ledger-toggle"
        aria-expanded={open}
        aria-controls={panelId}
        onClick={onToggle}
      >
        <span className="ledger-status">
          <StatusMark status={m.status} />
          {m.status_since && <span className="since">since {formatTimestamp(m.status_since)}</span>}
        </span>
        <span className="ledger-paper">
          <span className="ledger-journal">{m.journal}</span>
          <span className="figures">{m.external_id ?? "No ID"}</span>
        </span>
        <span className="ledger-due">
          {m.deadline && days !== null ? (
            <>
              <span className={isUrgent(days) ? "due urgent" : "due"}>{relativeDue(days)}</span>
              <span className="since">
                {m.requested_action ?? "Due"}, {longDate(m.deadline)}
              </span>
            </>
          ) : (
            <span className="since">No open deadline</span>
          )}
          {m.pending_reviews > 0 && (
            <span className="pending-note">
              {m.pending_reviews === 1
                ? "1 update to review"
                : `${m.pending_reviews} updates to review`}
            </span>
          )}
        </span>
        <ChevronDown className="chevron" aria-hidden="true" size={18} />
      </button>
      {open && (
        <div id={panelId}>
          <Timeline
            manuscriptId={m.id}
            // The summary changes after a correction; reload the steps with it.
            version={`${m.status}-${m.status_since}-${m.deadline ? longDate(m.deadline) : ""}`}
            onAmend={props.onAmend}
            onRemove={props.onRemove}
          />
          {props.others.length > 0 && (
            <MergeControl others={props.others} onMerge={props.onMerge} />
          )}
        </div>
      )}
    </li>
  );
}

function Timeline(props: {
  manuscriptId: number;
  version: string;
  onAmend: (eventId: number, edits: EventEdits) => Promise<void>;
  onRemove: (eventId: number) => Promise<void>;
}) {
  const { manuscriptId, version } = props;
  const [entries, setEntries] = useState<TimelineEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reloads, setReloads] = useState(0);

  useEffect(() => {
    api.timeline(manuscriptId).then(setEntries, (e) => setError(String(e)));
  }, [manuscriptId, version, reloads]);

  const reload = () => setReloads((n) => n + 1);

  if (error) {
    return (
      <p className="notice error" role="alert">
        The timeline could not be loaded: {error}
      </p>
    );
  }
  if (!entries) return null;

  return (
    <ol className="timeline" aria-label="Updates, newest first">
      {entries.map((e) => (
        <li key={e.event_id} className={e.review_state === "proposed" ? "step pending" : "step"}>
          <time className="step-date">{formatTimestamp(e.occurred_at)}</time>
          <Step
            entry={e}
            onAmend={async (edits) => {
              await props.onAmend(e.event_id, edits);
              reload();
            }}
            onRemove={async () => {
              await props.onRemove(e.event_id);
              reload();
            }}
          />
        </li>
      ))}
    </ol>
  );
}

/** One update; confirmed ones can be corrected or removed. */
function Step(props: {
  entry: TimelineEntry;
  onAmend: (edits: EventEdits) => Promise<void>;
  onRemove: () => Promise<void>;
}) {
  const { entry: e } = props;
  const [mode, setMode] = useState<"view" | "correct" | "remove">("view");
  const [busy, setBusy] = useState(false);
  const reviewed = e.review_state === "confirmed" || e.review_state === "edited";

  async function run(action: () => Promise<void>) {
    setBusy(true);
    try {
      await action();
      setMode("view");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="step-body">
      <p className="step-title">
        {e.status ? statusLabel(e.status) : "Update"}
        {e.review_state === "edited" && <span className="since"> (corrected by you)</span>}
        {e.review_state === "proposed" && <span className="since"> (waiting for review)</span>}
      </p>
      {e.original_phrase && <p className="step-phrase">{e.original_phrase}</p>}
      {e.deadline && <p className="since">Due {longDate(e.deadline)}</p>}
      {e.source && (
        <p className="since source">
          {e.source.account_alias}, {e.source.folder}
        </p>
      )}
      {reviewed && mode === "view" && (
        <div className="step-actions">
          <button className="quiet" onClick={() => setMode("correct")}>
            Correct
          </button>
          <button className="quiet" onClick={() => setMode("remove")}>
            Remove
          </button>
        </div>
      )}
      {mode === "correct" && (
        <EventForm
          status={e.status}
          deadline={e.deadline}
          action={e.requested_action}
          busy={busy}
          submitLabel="Save correction"
          onCancel={() => setMode("view")}
          onSave={(edits) => void run(() => props.onAmend(edits))}
        />
      )}
      {mode === "remove" && (
        <div className="decide">
          <span className="confirm-text">
            Remove this update from the timeline? The message won't be suggested again.
          </span>
          <button disabled={busy} onClick={() => void run(props.onRemove)}>
            Remove update
          </button>
          <button className="quiet" onClick={() => setMode("view")}>
            Keep it
          </button>
        </div>
      )}
    </div>
  );
}

/** Combines two records of the same paper; the chosen one keeps its name and ID. */
function MergeControl(props: {
  others: ManuscriptSummary[];
  onMerge: (intoId: number) => Promise<void>;
}) {
  const [target, setTarget] = useState<number | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const chosen = props.others.find((m) => m.id === target);
  const name = (m: ManuscriptSummary) => `${m.journal}, ${m.external_id ?? "no ID"}`;

  return (
    <div className="merge">
      <label className="field">
        <span>Same paper as another entry?</span>
        <select
          value={target ?? ""}
          onChange={(e) => {
            setTarget(e.target.value ? Number(e.target.value) : null);
            setConfirming(false);
          }}
        >
          <option value="">Choose a manuscript</option>
          {props.others.map((m) => (
            <option key={m.id} value={m.id}>
              {name(m)}
            </option>
          ))}
        </select>
      </label>
      {chosen &&
        (confirming ? (
          <div className="decide">
            <span className="confirm-text">
              Move every update here to {name(chosen)} and remove this entry? This can't be undone.
            </span>
            <button
              disabled={busy}
              onClick={async () => {
                setBusy(true);
                await props.onMerge(chosen.id);
                setBusy(false);
              }}
            >
              Merge
            </button>
            <button className="quiet" onClick={() => setConfirming(false)}>
              Cancel
            </button>
          </div>
        ) : (
          <div className="decide">
            <button className="secondary" onClick={() => setConfirming(true)}>
              Merge into {chosen.external_id ?? chosen.journal}
            </button>
          </div>
        ))}
    </div>
  );
}
