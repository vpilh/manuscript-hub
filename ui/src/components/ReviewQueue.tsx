import { useState } from "react";

import type {
  EditorialStatus,
  ManuscriptSummary,
  QueueItem,
  ReviewDecision,
} from "../lib/api";
import {
  ALL_STATUSES,
  confidenceLabel,
  daysUntil,
  formatTimestamp,
  isoDate,
  parseIsoDate,
  statusLabel,
} from "../lib/format";

interface Props {
  items: QueueItem[];
  manuscripts: ManuscriptSummary[];
  onDecide: (eventId: number, decision: ReviewDecision) => Promise<void>;
}

export function ReviewQueue({ items, manuscripts, onDecide }: Props) {
  if (items.length === 0) {
    return (
      <p className="empty muted">
        Nothing to review. New journal mail shows up here before anything is saved.
      </p>
    );
  }
  return (
    <ul className="cards">
      {items.map((item) => (
        <ReviewCard
          key={item.event_id}
          item={item}
          manuscripts={manuscripts}
          onDecide={(decision) => onDecide(item.event_id, decision)}
        />
      ))}
    </ul>
  );
}

function ReviewCard(props: {
  item: QueueItem;
  manuscripts: ManuscriptSummary[];
  onDecide: (decision: ReviewDecision) => Promise<void>;
}) {
  const { item, manuscripts, onDecide } = props;
  const { proposal, match_kind, match_rationale, suggested_manuscript } = item;
  const { status, deadline, manuscript_id, journal, requested_action, uncertainty } = proposal;
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);

  // A shared ID needs an explicit choice among the manuscripts that use it.
  const candidates =
    match_kind === "ambiguous"
      ? manuscripts.filter((m) => m.external_id === manuscript_id?.value)
      : [];
  const [choice, setChoice] = useState<number | null>(null);
  const needsChoice = match_kind === "ambiguous" && choice === null;

  async function decide(decision: ReviewDecision) {
    setBusy(true);
    await onDecide(decision);
    setBusy(false);
  }

  const days = deadline ? daysUntil(deadline.value, new Date()) : null;

  return (
    <li className="card">
      <div className="card-head">
        <span className="status">{status ? statusLabel(status.value) : "No status found"}</span>
        <span className="mono">{manuscript_id?.value ?? "No manuscript ID"}</span>
      </div>
      <p className="journal">
        {journal?.value ?? "Unknown journal"} · received{" "}
        {formatTimestamp(proposal.source.received_at)}
      </p>

      <dl>
        {deadline && (
          <>
            <dt>Deadline</dt>
            <dd>
              {isoDate(deadline.value)}
              {days !== null && (
                <span className="muted"> · {days >= 0 ? `in ${days} days` : `${-days} days ago`}</span>
              )}
              <Confidence value={deadline.confidence} />
            </dd>
          </>
        )}
        {requested_action && (
          <>
            <dt>Action</dt>
            <dd>{requested_action.value}</dd>
          </>
        )}
        {status && (
          <>
            <dt>Evidence</dt>
            <dd>
              “{status.original}”
              <Confidence value={status.confidence} />
            </dd>
          </>
        )}
        <dt>Match</dt>
        <dd>
          {suggested_manuscript && (
            <strong>
              {suggested_manuscript.journal}
              {suggested_manuscript.external_id && ` · ${suggested_manuscript.external_id}`}.{" "}
            </strong>
          )}
          <span className="muted">{match_rationale}</span>
        </dd>
      </dl>

      {uncertainty.length > 0 && (
        <ul className="uncertainty">
          {uncertainty.map((reason) => (
            <li key={reason}>{reason}</li>
          ))}
        </ul>
      )}

      {candidates.length > 0 && (
        <label className="field">
          Which manuscript is this about?
          <select
            value={choice ?? ""}
            onChange={(e) => setChoice(e.target.value ? Number(e.target.value) : null)}
          >
            <option value="">Choose…</option>
            {candidates.map((m) => (
              <option key={m.id} value={m.id}>
                {m.journal} · {m.external_id}
              </option>
            ))}
          </select>
        </label>
      )}

      {editing ? (
        <EditForm
          item={item}
          busy={busy}
          onCancel={() => setEditing(false)}
          onSave={(edits) => decide({ kind: "confirm", manuscript_id: choice, edits })}
        />
      ) : (
        <div className="actions">
          <button
            disabled={busy || needsChoice}
            onClick={() => decide({ kind: "confirm", manuscript_id: choice })}
          >
            Confirm
          </button>
          <button className="secondary" disabled={busy || needsChoice} onClick={() => setEditing(true)}>
            Edit…
          </button>
          <button className="quiet" disabled={busy} onClick={() => decide({ kind: "ignore" })}>
            Ignore
          </button>
          <button className="quiet" disabled={busy} onClick={() => decide({ kind: "unrelated" })}>
            Not journal mail
          </button>
        </div>
      )}
    </li>
  );
}

function EditForm(props: {
  item: QueueItem;
  busy: boolean;
  onCancel: () => void;
  onSave: (edits: {
    status: EditorialStatus | null;
    deadline: ReturnType<typeof parseIsoDate>;
    requested_action: string | null;
  }) => void;
}) {
  const { proposal } = props.item;
  const [status, setStatus] = useState<string>(proposal.status?.value ?? "");
  const [deadline, setDeadline] = useState(
    proposal.deadline ? isoDate(proposal.deadline.value) : "",
  );
  const [action, setAction] = useState(proposal.requested_action?.value ?? "");
  const deadlineInvalid = deadline !== "" && parseIsoDate(deadline) === null;

  return (
    <form
      className="edit"
      onSubmit={(e) => {
        e.preventDefault();
        props.onSave({
          status: (status || null) as EditorialStatus | null,
          deadline: parseIsoDate(deadline),
          requested_action: action.trim() || null,
        });
      }}
    >
      <label className="field">
        Status
        <select value={status} onChange={(e) => setStatus(e.target.value)}>
          <option value="">None</option>
          {ALL_STATUSES.map((s) => (
            <option key={s} value={s}>
              {statusLabel(s)}
            </option>
          ))}
        </select>
      </label>
      <label className="field">
        Deadline
        <input type="date" value={deadline} onChange={(e) => setDeadline(e.target.value)} />
      </label>
      <label className="field">
        Requested action
        <input type="text" value={action} onChange={(e) => setAction(e.target.value)} />
      </label>
      <div className="actions">
        <button type="submit" disabled={props.busy || deadlineInvalid}>
          Save and confirm
        </button>
        <button type="button" className="quiet" onClick={props.onCancel}>
          Cancel
        </button>
      </div>
    </form>
  );
}

function Confidence({ value }: { value: number }) {
  const label = confidenceLabel(value);
  return <span className={`confidence ${label}`}>{label}</span>;
}
