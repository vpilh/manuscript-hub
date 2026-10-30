import { useId, useState } from "react";

import type { CalendarDate, EditorialStatus, EventEdits } from "../lib/api";
import { ALL_STATUSES, isoDate, parseIsoDate, statusLabel } from "../lib/format";

/** Status, deadline and to-do of one update, for correcting what was read. */
export function EventForm(props: {
  status: EditorialStatus | null;
  deadline: CalendarDate | null;
  action: string | null;
  busy: boolean;
  submitLabel: string;
  onCancel: () => void;
  onSave: (edits: EventEdits) => void;
}) {
  const [status, setStatus] = useState<string>(props.status ?? "");
  const [deadline, setDeadline] = useState(props.deadline ? isoDate(props.deadline) : "");
  const [action, setAction] = useState(props.action ?? "");
  const deadlineInvalid = deadline !== "" && parseIsoDate(deadline) === null;
  const deadlineError = useId();

  return (
    <form
      className="corrections"
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
        <span>Status</span>
        <select value={status} onChange={(e) => setStatus(e.target.value)}>
          <option value="">No status</option>
          {ALL_STATUSES.map((s) => (
            <option key={s} value={s}>
              {statusLabel(s)}
            </option>
          ))}
        </select>
      </label>
      <label className="field">
        <span>Deadline</span>
        <input
          type="date"
          value={deadline}
          aria-invalid={deadlineInvalid}
          aria-describedby={deadlineInvalid ? deadlineError : undefined}
          onChange={(e) => setDeadline(e.target.value)}
        />
        {deadlineInvalid && (
          <span id={deadlineError} className="field-error">
            Enter a real calendar date, or leave it empty.
          </span>
        )}
      </label>
      <label className="field">
        <span>What to do</span>
        <input
          type="text"
          value={action}
          placeholder="e.g. Submit revised manuscript"
          onChange={(e) => setAction(e.target.value)}
        />
      </label>
      <div className="decide">
        <button type="submit" disabled={props.busy || deadlineInvalid}>
          {props.submitLabel}
        </button>
        <button type="button" className="quiet" onClick={props.onCancel}>
          Cancel
        </button>
      </div>
    </form>
  );
}
