import { useId, useState, type FormEvent, type ReactNode } from "react";

import type { ImportSummary, Imported, Saved } from "../lib/api";

const MIN_PASSWORD = 10;

interface Props {
  onExportCsv: () => Promise<Saved>;
  onExportJson: () => Promise<Saved>;
  onBackup: (password: string) => Promise<Saved>;
  onImportJson: () => Promise<Imported>;
  onRestore: (password: string) => Promise<Imported>;
  onDeleteAll: () => Promise<void>;
}

function importSentence(summary: ImportSummary): string {
  const { manuscripts_added: m, updates_added: u, updates_skipped: s } = summary;
  if (m === 0 && u === 0) return "Everything in that file was already here; nothing changed.";
  const parts = [
    `${m} new ${m === 1 ? "manuscript" : "manuscripts"}`,
    `${u} ${u === 1 ? "update" : "updates"} added`,
  ];
  if (s > 0) parts.push(`${s} already here`);
  return `Imported: ${parts.join(", ")}.`;
}

/** Runs an action and reports its outcome next to the button that started it. */
function useOutcome() {
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  async function run(action: () => Promise<string | null>) {
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      setMessage(await action());
    } catch (e) {
      setError(String(e));
    }
    setBusy(false);
  }
  const outcome = (
    <>
      {message && (
        <p className="notice" role="status">
          {message}
        </p>
      )}
      {error && (
        <p className="notice error" role="alert">
          {error}
        </p>
      )}
    </>
  );
  return { run, busy, outcome };
}

const savedTo = (saved: Saved) => (saved.path ? `Saved to ${saved.path}` : null);

export function YourData(props: Props) {
  return (
    <div className="data-page">
      <Section title="Export">
        <p className="hint">
          A spreadsheet with one row per manuscript, or a complete export with every confirmed
          update that Manuscript Hub can import again. Updates still waiting for review aren't
          included.
        </p>
        <Exports {...props} />
      </Section>
      <Section title="Backup">
        <p className="hint">
          A backup file locked with a password you choose. It can be restored on any computer,
          also when this one is lost. Without the password the backup can't be opened, and the
          password isn't stored anywhere.
        </p>
        <Backup onBackup={props.onBackup} />
      </Section>
      <Section title="Restore or import">
        <p className="hint">
          Adds what a backup or export holds to what is already here. Nothing is removed, and
          updates that are already here are skipped.
        </p>
        <Restore onRestore={props.onRestore} onImportJson={props.onImportJson} />
      </Section>
      <Section title="Delete all data">
        <p className="hint">
          Removes every manuscript, update, connected mailbox, sender rule and the stored mailbox
          passwords from this computer. Your mailboxes themselves aren't touched. Make a backup
          first if you might want the data again.
        </p>
        <DeleteAll onDeleteAll={props.onDeleteAll} />
      </Section>
    </div>
  );
}

function Section(props: { title: string; children: ReactNode }) {
  const id = useId();
  return (
    <section className="data-section" aria-labelledby={id}>
      <h2 id={id}>{props.title}</h2>
      {props.children}
    </section>
  );
}

function Exports(props: Pick<Props, "onExportCsv" | "onExportJson">) {
  const { run, busy, outcome } = useOutcome();
  return (
    <>
      <div className="decide">
        <button
          className="secondary"
          disabled={busy}
          onClick={() => run(async () => savedTo(await props.onExportCsv()))}
        >
          Save spreadsheet (CSV)
        </button>
        <button
          className="secondary"
          disabled={busy}
          onClick={() => run(async () => savedTo(await props.onExportJson()))}
        >
          Save complete export (JSON)
        </button>
      </div>
      {outcome}
    </>
  );
}

function Backup(props: Pick<Props, "onBackup">) {
  const [password, setPassword] = useState("");
  const [repeat, setRepeat] = useState("");
  const { run, busy, outcome } = useOutcome();
  const tooShort = password.length > 0 && password.length < MIN_PASSWORD;
  const mismatch = repeat.length > 0 && repeat !== password;
  const ready = password.length >= MIN_PASSWORD && repeat === password;

  function submit(e: FormEvent) {
    e.preventDefault();
    void run(async () => {
      const saved = await props.onBackup(password);
      if (saved.path) {
        setPassword("");
        setRepeat("");
      }
      return savedTo(saved);
    });
  }

  return (
    <form onSubmit={submit}>
      <div className="field-row">
        <label className="field grow">
          <span>Backup password</span>
          <input
            type="password"
            value={password}
            autoComplete="new-password"
            aria-invalid={tooShort}
            onChange={(e) => setPassword(e.target.value)}
          />
          {tooShort && (
            <span className="field-error">Use at least {MIN_PASSWORD} characters.</span>
          )}
        </label>
        <label className="field grow">
          <span>Repeat it</span>
          <input
            type="password"
            value={repeat}
            autoComplete="new-password"
            aria-invalid={mismatch}
            onChange={(e) => setRepeat(e.target.value)}
          />
          {mismatch && <span className="field-error">The two passwords differ.</span>}
        </label>
      </div>
      <div className="decide">
        <button type="submit" disabled={busy || !ready}>
          Save backup
        </button>
      </div>
      {outcome}
    </form>
  );
}

function Restore(props: Pick<Props, "onRestore" | "onImportJson">) {
  const [password, setPassword] = useState("");
  const { run, busy, outcome } = useOutcome();
  const done = (imported: Imported) =>
    imported.summary ? importSentence(imported.summary) : null;

  function submit(e: FormEvent) {
    e.preventDefault();
    void run(async () => {
      const imported = await props.onRestore(password);
      if (imported.summary) setPassword("");
      return done(imported);
    });
  }

  return (
    <>
      <form onSubmit={submit}>
        <label className="field">
          <span>Password of the backup</span>
          <input
            type="password"
            value={password}
            autoComplete="off"
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
        <div className="decide">
          <button type="submit" disabled={busy || !password}>
            Choose backup and restore
          </button>
          <button
            type="button"
            className="quiet"
            disabled={busy}
            onClick={() => run(async () => done(await props.onImportJson()))}
          >
            Import a JSON export instead
          </button>
        </div>
      </form>
      {outcome}
    </>
  );
}

function DeleteAll(props: Pick<Props, "onDeleteAll">) {
  const [confirming, setConfirming] = useState(false);
  const { run, busy, outcome } = useOutcome();
  return (
    <>
      {confirming ? (
        <div className="decide">
          <span className="confirm-text">
            Delete all manuscripts, updates, mailboxes and rules from this computer? This can't be
            undone.
          </span>
          <button
            className="danger"
            disabled={busy}
            onClick={() =>
              run(async () => {
                await props.onDeleteAll();
                setConfirming(false);
                return "All data on this computer was deleted.";
              })
            }
          >
            Delete everything
          </button>
          <button className="quiet" onClick={() => setConfirming(false)}>
            Keep my data
          </button>
        </div>
      ) : (
        <div className="decide">
          <button className="secondary" onClick={() => setConfirming(true)}>
            Delete all data…
          </button>
        </div>
      )}
      {outcome}
    </>
  );
}
