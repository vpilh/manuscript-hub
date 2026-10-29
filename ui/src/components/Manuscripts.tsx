import { ChevronDown } from "lucide-react";
import { useEffect, useId, useState } from "react";

import { api, type ManuscriptSummary, type TimelineEntry } from "../lib/api";
import { daysUntil, formatTimestamp, isUrgent, longDate, relativeDue, statusLabel } from "../lib/format";
import { StatusMark } from "./StatusMark";

interface Props {
  manuscripts: ManuscriptSummary[];
  initialOpenId: number | null;
  onOpenReview: () => void;
}

export function Manuscripts({ manuscripts, initialOpenId, onOpenReview }: Props) {
  const [openId, setOpenId] = useState<number | null>(initialOpenId);

  if (manuscripts.length === 0) {
    return (
      <div className="empty">
        <p className="empty-title">No manuscripts tracked yet.</p>
        <p>Add a message from the review list and its manuscript starts here, with every update in order.</p>
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
        />
      ))}
    </ul>
  );
}

function LedgerRow(props: {
  manuscript: ManuscriptSummary;
  open: boolean;
  onToggle: () => void;
}) {
  const { manuscript: m, open, onToggle } = props;
  const panelId = useId();
  const days = m.deadline ? daysUntil(m.deadline, new Date()) : null;

  return (
    <li className={open ? "ledger-row open" : "ledger-row"}>
      <button className="ledger-toggle" aria-expanded={open} aria-controls={panelId} onClick={onToggle}>
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
              {m.pending_reviews === 1 ? "1 update to review" : `${m.pending_reviews} updates to review`}
            </span>
          )}
        </span>
        <ChevronDown className="chevron" aria-hidden="true" size={18} />
      </button>
      {open && (
        <div id={panelId}>
          <Timeline manuscriptId={m.id} />
        </div>
      )}
    </li>
  );
}

function Timeline({ manuscriptId }: { manuscriptId: number }) {
  const [entries, setEntries] = useState<TimelineEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.timeline(manuscriptId).then(setEntries, (e) => setError(String(e)));
  }, [manuscriptId]);

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
          <div className="step-body">
            <p className="step-title">
              {e.status ? statusLabel(e.status) : "Update"}
              {e.review_state === "edited" && <span className="since"> (corrected by you)</span>}
              {e.review_state === "proposed" && <span className="since"> (waiting for review)</span>}
            </p>
            {e.original_phrase && <p className="step-quote">“{e.original_phrase}”</p>}
            {e.deadline && <p className="since">Due {longDate(e.deadline)}</p>}
            {e.source && (
              <p className="since source">
                {e.source.account_alias}, {e.source.folder}
              </p>
            )}
          </div>
        </li>
      ))}
    </ol>
  );
}
