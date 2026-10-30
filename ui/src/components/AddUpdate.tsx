import { useId, useState, type FormEvent } from "react";

import type {
  EditorialStatus,
  ManualTarget,
  ManualUpdate,
  ManuscriptSummary,
  PasteOutcome,
} from "../lib/api";
import { ALL_STATUSES, isoDate, parseIsoDate, statusLabel } from "../lib/format";

type Mode = "manual" | "paste";

/** Adds news that didn't arrive in a connected mailbox: typed in, or pasted. */
export function AddUpdate(props: {
  manuscripts: ManuscriptSummary[];
  onAddManual: (target: ManualTarget, update: ManualUpdate) => Promise<number>;
  onPaste: (text: string) => Promise<PasteOutcome>;
  onOpenReview: () => void;
  onDone: (manuscriptId: number | null) => void;
}) {
  const [mode, setMode] = useState<Mode>("manual");
  const titleId = useId();

  return (
    <section className="add-update" aria-labelledby={titleId}>
      <h2 id={titleId}>Add an update</h2>
      <fieldset className="choice">
        <legend>How</legend>
        <label>
          <input
            type="radio"
            name="add-mode"
            checked={mode === "manual"}
            onChange={() => setMode("manual")}
          />
          Enter it by hand
        </label>
        <label>
          <input
            type="radio"
            name="add-mode"
            checked={mode === "paste"}
            onChange={() => setMode("paste")}
          />
          Paste an email
        </label>
      </fieldset>
      {mode === "manual" ? (
        <ManualForm
          manuscripts={props.manuscripts}
          onAdd={props.onAddManual}
          onDone={props.onDone}
        />
      ) : (
        <PasteForm
          onPaste={props.onPaste}
          onOpenReview={props.onOpenReview}
          onCancel={() => props.onDone(null)}
        />
      )}
    </section>
  );
}

function ManualForm(props: {
  manuscripts: ManuscriptSummary[];
  onAdd: (target: ManualTarget, update: ManualUpdate) => Promise<number>;
  onDone: (manuscriptId: number | null) => void;
}) {
  const [target, setTarget] = useState<string>(
    props.manuscripts.length > 0 ? String(props.manuscripts[0].id) : "new",
  );
  const [journal, setJournal] = useState("");
  const [externalId, setExternalId] = useState("");
  const [title, setTitle] = useState("");
  const [date, setDate] = useState(() => {
    const now = new Date();
    return isoDate({ year: now.getFullYear(), month: now.getMonth() + 1, day: now.getDate() });
  });
  const [status, setStatus] = useState("");
  const [deadline, setDeadline] = useState("");
  const [action, setAction] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isNew = target === "new";
  const dateValue = parseIsoDate(date);
  const deadlineInvalid = deadline !== "" && parseIsoDate(deadline) === null;
  const ready =
    dateValue !== null &&
    !deadlineInvalid &&
    (!isNew || journal.trim() !== "") &&
    (status !== "" || deadline !== "" || action.trim() !== "");

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!dateValue) return;
    setBusy(true);
    setError(null);
    try {
      const id = await props.onAdd(
        isNew
          ? {
              kind: "new",
              journal: journal.trim(),
              external_id: externalId.trim() || null,
              title: title.trim() || null,
            }
          : { kind: "existing", manuscript_id: Number(target) },
        {
          occurred_on: dateValue,
          status: (status || null) as EditorialStatus | null,
          deadline: parseIsoDate(deadline),
          requested_action: action.trim() || null,
        },
      );
      props.onDone(id);
    } catch (err) {
      setError(String(err));
    }
    setBusy(false);
  }

  return (
    <form onSubmit={submit}>
      <label className="field">
        <span>Manuscript</span>
        <select value={target} onChange={(e) => setTarget(e.target.value)}>
          {props.manuscripts.map((m) => (
            <option key={m.id} value={m.id}>
              {m.journal}, {m.external_id ?? "no ID"}
            </option>
          ))}
          <option value="new">A new manuscript</option>
        </select>
      </label>
      {isNew && (
        <>
          <label className="field">
            <span>Journal</span>
            <input value={journal} onChange={(e) => setJournal(e.target.value)} />
          </label>
          <div className="field-row">
            <label className="field">
              <span>Manuscript ID (if any)</span>
              <input
                value={externalId}
                onChange={(e) => setExternalId(e.target.value)}
                spellCheck={false}
              />
            </label>
            <label className="field grow">
              <span>Title (if you like)</span>
              <input value={title} onChange={(e) => setTitle(e.target.value)} />
            </label>
          </div>
        </>
      )}
      <div className="field-row">
        <label className="field">
          <span>Date</span>
          <input type="date" value={date} onChange={(e) => setDate(e.target.value)} />
        </label>
        <label className="field grow">
          <span>Status</span>
          <select value={status} onChange={(e) => setStatus(e.target.value)}>
            <option value="">No change</option>
            {ALL_STATUSES.map((s) => (
              <option key={s} value={s}>
                {statusLabel(s)}
              </option>
            ))}
          </select>
        </label>
      </div>
      <div className="field-row">
        <label className="field">
          <span>Deadline</span>
          <input
            type="date"
            value={deadline}
            aria-invalid={deadlineInvalid}
            onChange={(e) => setDeadline(e.target.value)}
          />
        </label>
        <label className="field grow">
          <span>What to do</span>
          <input
            value={action}
            placeholder="e.g. Submit revised manuscript"
            onChange={(e) => setAction(e.target.value)}
          />
        </label>
      </div>
      {error && (
        <p className="notice error" role="alert">
          {error}
        </p>
      )}
      <div className="decide">
        <button type="submit" disabled={busy || !ready}>
          Save update
        </button>
        <button type="button" className="quiet" onClick={() => props.onDone(null)}>
          Cancel
        </button>
      </div>
    </form>
  );
}

const PASTE_RESULTS: Record<PasteOutcome, string> = {
  proposed: "Added to Review. Check what was read before saving it.",
  duplicate: "This message is already in Manuscript Hub.",
  nothing_found:
    "No manuscript ID or editorial status found in that text. Enter the update by hand instead.",
};

function PasteForm(props: {
  onPaste: (text: string) => Promise<PasteOutcome>;
  onOpenReview: () => void;
  onCancel: () => void;
}) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [outcome, setOutcome] = useState<PasteOutcome | null>(null);
  const [error, setError] = useState<string | null>(null);
  const hintId = useId();

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    setOutcome(null);
    try {
      const result = await props.onPaste(text);
      setOutcome(result);
      if (result !== "nothing_found") setText("");
    } catch (err) {
      setError(String(err));
    }
    setBusy(false);
  }

  return (
    <form onSubmit={submit}>
      <label className="field paste">
        <span>Message</span>
        <textarea
          value={text}
          rows={10}
          aria-describedby={hintId}
          onChange={(e) => setText(e.target.value)}
        />
      </label>
      <p className="hint" id={hintId}>
        Paste the whole message, or just its text. In Gmail, "Show original" gives the full source.
        The text is read once and not kept.
      </p>
      {outcome && (
        <p className="notice" role="status">
          {PASTE_RESULTS[outcome]}{" "}
          {outcome === "proposed" && (
            <button type="button" className="link" onClick={props.onOpenReview}>
              Go to review
            </button>
          )}
        </p>
      )}
      {error && (
        <p className="notice error" role="alert">
          {error}
        </p>
      )}
      <div className="decide">
        <button type="submit" disabled={busy || !text.trim()}>
          Read this message
        </button>
        <button type="button" className="quiet" onClick={props.onCancel}>
          Close
        </button>
      </div>
    </form>
  );
}
