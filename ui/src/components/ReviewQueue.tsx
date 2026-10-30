import { TriangleAlert } from "lucide-react";
import { useId, useState } from "react";

import type { ManuscriptSummary, QueueItem, ReviewDecision } from "../lib/api";
import {
  confidenceWord,
  daysUntil,
  formatTimestamp,
  isUrgent,
  longDate,
  relativeDue,
} from "../lib/format";
import { EventForm } from "./EventForm";
import { StatusMark } from "./StatusMark";

interface Props {
  items: QueueItem[];
  manuscripts: ManuscriptSummary[];
  scanning: boolean;
  hasMailboxes: boolean;
  onScan: () => void;
  onOpenMailboxes: () => void;
  onDecide: (eventId: number, decision: ReviewDecision) => Promise<void>;
}

export function ReviewQueue(props: Props) {
  const { items, manuscripts, scanning, hasMailboxes, onScan, onOpenMailboxes, onDecide } = props;
  if (items.length === 0) {
    return (
      <div className="empty">
        <p className="empty-title">Nothing waiting for a decision.</p>
        <p>
          When journal mail arrives, each message shows up here with the words it was read from.
          Nothing is saved until you decide.
        </p>
        {hasMailboxes ? (
          <button className="secondary" onClick={onOpenMailboxes}>
            Check your mailboxes
          </button>
        ) : (
          <div className="decide">
            <button onClick={onOpenMailboxes}>Connect a mailbox</button>
            <button className="secondary" onClick={onScan} disabled={scanning}>
              {scanning ? "Scanning the demo mailbox…" : "Try the demo mailbox"}
            </button>
          </div>
        )}
      </div>
    );
  }
  return (
    <ol className="proposals" aria-label="Messages to review">
      {items.map((item) => (
        <Proposal
          key={item.event_id}
          item={item}
          manuscripts={manuscripts}
          onDecide={(decision) => onDecide(item.event_id, decision)}
        />
      ))}
    </ol>
  );
}

function Proposal(props: {
  item: QueueItem;
  manuscripts: ManuscriptSummary[];
  onDecide: (decision: ReviewDecision) => Promise<void>;
}) {
  const { item, manuscripts, onDecide } = props;
  const { proposal, match_kind, match_rationale, suggested_manuscript } = item;
  const { status, deadline, manuscript_id, journal, requested_action, uncertainty } = proposal;
  const headingId = useId();
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);

  // A shared ID needs an explicit choice among the manuscripts that use it.
  const candidates =
    match_kind === "ambiguous"
      ? manuscripts.filter((m) => m.external_id === manuscript_id?.value)
      : [];
  const [choice, setChoice] = useState<number | null>(null);
  const chosen = candidates.find((m) => m.id === choice);
  const needsChoice = match_kind === "ambiguous" && !chosen;

  const target = chosen ?? suggested_manuscript;
  const targetName = target ? (target.external_id ?? target.journal) : null;
  const addLabel = targetName
    ? `Add to ${targetName}`
    : manuscript_id
      ? `Start tracking ${manuscript_id.value}`
      : "Start tracking";

  async function decide(decision: ReviewDecision) {
    setBusy(true);
    await onDecide(decision);
    setBusy(false);
  }

  const days = deadline ? daysUntil(deadline.value, new Date()) : null;

  return (
    <li className="proposal" aria-labelledby={headingId}>
      <figure className="evidence">
        {status ? (
          <blockquote id={headingId}>“{status.original}”</blockquote>
        ) : (
          <blockquote id={headingId} className="no-evidence">
            No editorial decision found in this message.
          </blockquote>
        )}
        <figcaption>
          From {journal?.value ?? "an unknown sender"}, received{" "}
          {formatTimestamp(proposal.source.received_at)}
        </figcaption>
        {proposal.reason && <p className="reason">Shown because: {proposal.reason}</p>}
      </figure>

      <dl className="facts">
        <dt>Status</dt>
        <dd>
          <StatusMark status={status?.value ?? null} />
          {status && <span className="confidence">{confidenceWord(status.confidence)}</span>}
        </dd>

        <dt>Manuscript</dt>
        <dd className="figures">{manuscript_id?.value ?? "No ID in the message"}</dd>

        {deadline && days !== null && (
          <>
            <dt>Due</dt>
            <dd>
              <span className={isUrgent(days) ? "due urgent" : "due"}>
                {longDate(deadline.value)}, {relativeDue(days)}
              </span>
              <span className="confidence">{confidenceWord(deadline.confidence)}</span>
            </dd>
          </>
        )}

        {requested_action && (
          <>
            <dt>To do</dt>
            <dd>{requested_action.value}</dd>
          </>
        )}

        <dt>Adds to</dt>
        <dd>
          {target ? (
            <span className="target">
              {target.journal}, {target.external_id ?? "no ID"}
            </span>
          ) : (
            <span className="target">Starts a new manuscript</span>
          )}
          {/* An exact ID match explains itself; anything weaker says why. */}
          {match_kind !== "exact_id" && !chosen && (
            <span className="rationale">{match_rationale}</span>
          )}
        </dd>
      </dl>

      {uncertainty.length > 0 && (
        <ul className="doubts" aria-label="Check before saving">
          {uncertainty.map((reason) => (
            <li key={reason}>
              <TriangleAlert aria-hidden="true" size={16} />
              {reason}
            </li>
          ))}
        </ul>
      )}

      {candidates.length > 0 && (
        <label className="field">
          <span>Which manuscript is this about?</span>
          <select
            value={choice ?? ""}
            onChange={(e) => setChoice(e.target.value ? Number(e.target.value) : null)}
          >
            <option value="">Choose a manuscript</option>
            {candidates.map((m) => (
              <option key={m.id} value={m.id}>
                {m.journal}, {m.external_id}
              </option>
            ))}
          </select>
        </label>
      )}

      {editing ? (
        <EventForm
          status={proposal.status?.value ?? null}
          deadline={proposal.deadline?.value ?? null}
          action={proposal.requested_action?.value ?? null}
          busy={busy}
          submitLabel={targetName ? "Save and add" : "Save and start tracking"}
          onCancel={() => setEditing(false)}
          onSave={(edits) => decide({ kind: "confirm", manuscript_id: choice, edits })}
        />
      ) : (
        <div className="decide">
          <button
            disabled={busy || needsChoice}
            onClick={() => decide({ kind: "confirm", manuscript_id: choice })}
          >
            {needsChoice ? "Choose a manuscript first" : addLabel}
          </button>
          <button className="secondary" disabled={busy} onClick={() => setEditing(true)}>
            Correct details
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
