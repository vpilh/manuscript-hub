import { useEffect, useState } from "react";

import { api, type ManuscriptSummary, type TimelineEntry } from "../lib/api";
import { daysUntil, formatTimestamp, isoDate, statusLabel } from "../lib/format";

export function Manuscripts({ manuscripts }: { manuscripts: ManuscriptSummary[] }) {
  const [openId, setOpenId] = useState<number | null>(null);

  if (manuscripts.length === 0) {
    return (
      <p className="empty muted">
        No manuscripts yet. Confirm an item in the review queue to start tracking it.
      </p>
    );
  }

  return (
    <ul className="cards">
      {manuscripts.map((m) => {
        const days = m.deadline ? daysUntil(m.deadline, new Date()) : null;
        const open = openId === m.id;
        return (
          <li key={m.id} className="card">
            <button
              className="row-button"
              aria-expanded={open}
              onClick={() => setOpenId(open ? null : m.id)}
            >
              <div className="card-head">
                <span className="status">{m.status ? statusLabel(m.status) : "No status yet"}</span>
                <span className="mono">{m.external_id ?? "No manuscript ID"}</span>
              </div>
              <p className="journal">
                {m.journal}
                {m.status_since && ` · since ${formatTimestamp(m.status_since)}`}
              </p>
              {m.deadline && (
                <p className={days !== null && days < 0 ? "deadline overdue" : "deadline"}>
                  {m.requested_action ?? "Due"} by {isoDate(m.deadline)}
                  {days !== null && (days >= 0 ? ` · in ${days} days` : ` · ${-days} days overdue`)}
                </p>
              )}
              {m.pending_reviews > 0 && (
                <p className="muted small">{m.pending_reviews} update(s) awaiting review</p>
              )}
            </button>
            {open && <Timeline manuscriptId={m.id} />}
          </li>
        );
      })}
    </ul>
  );
}

function Timeline({ manuscriptId }: { manuscriptId: number }) {
  const [entries, setEntries] = useState<TimelineEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.timeline(manuscriptId).then(setEntries, (e) => setError(String(e)));
  }, [manuscriptId]);

  if (error) return <p className="notice error">{error}</p>;
  if (!entries) return null;

  return (
    <ol className="timeline">
      {entries.map((e) => (
        <li key={e.event_id} className={e.review_state === "proposed" ? "pending" : undefined}>
          <span className="when">{formatTimestamp(e.occurred_at)}</span>
          <span>
            {e.status ? statusLabel(e.status) : "Update"}
            {e.deadline && ` · due ${isoDate(e.deadline)}`}
            {e.review_state === "edited" && <span className="muted"> · edited</span>}
            {e.review_state === "proposed" && <span className="muted"> · awaiting review</span>}
          </span>
          {e.source && (
            <span className="muted small source">
              {e.source.account_alias} · {e.source.folder} · <span className="mono">{e.source.message_id}</span>
            </span>
          )}
        </li>
      ))}
    </ol>
  );
}
