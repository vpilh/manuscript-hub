import { useEffect, useState } from "react";

import { api, inDesktopShell, type AppInfo, type ProposedEvent } from "./lib/api";
import { confidenceLabel, daysUntil, isoDate, statusLabel } from "./lib/format";

export function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [events, setEvents] = useState<ProposedEvent[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [scanning, setScanning] = useState(false);
  const desktop = inDesktopShell();

  useEffect(() => {
    if (desktop) api.appInfo().then(setInfo, (e) => setError(String(e)));
  }, [desktop]);

  async function scan() {
    setScanning(true);
    setError(null);
    try {
      setEvents(await api.scanDemoMailbox());
    } catch (e) {
      setError(String(e));
    } finally {
      setScanning(false);
    }
  }

  return (
    <main className="shell">
      <header className="masthead">
        <div>
          <h1>Manuscript Hub</h1>
          <p className="muted">
            Local-first submission tracking. Mail is read on this computer and never leaves it.
          </p>
        </div>
        {info && (
          <span className="muted small">
            v{info.version} · parser {info.parser_version}
          </span>
        )}
      </header>

      {!desktop ? (
        <p className="notice">
          This is the browser preview. Run <code>npm run tauri dev</code> to start the desktop app.
        </p>
      ) : (
        <section>
          <div className="toolbar">
            <h2>Review queue</h2>
            <button onClick={scan} disabled={scanning}>
              {scanning ? "Scanning…" : "Scan demo mailbox"}
            </button>
          </div>
          <p className="muted small">
            The demo mailbox is synthetic. Connecting real accounts arrives in later releases.
          </p>
          {error && <p className="notice error">{error}</p>}
          {events && events.length === 0 && <p className="muted">No journal mail found.</p>}
          {events && events.length > 0 && (
            <ul className="events">
              {events.map((event) => (
                <EventCard key={event.source.message_id} event={event} />
              ))}
            </ul>
          )}
        </section>
      )}
    </main>
  );
}

function EventCard({ event }: { event: ProposedEvent }) {
  const { status, deadline, manuscript_id, journal, requested_action, uncertainty } = event;
  const days = deadline ? daysUntil(deadline.value, new Date()) : null;

  return (
    <li className="event">
      <div className="event-head">
        <span className={`status ${status ? status.value : "unknown"}`}>
          {status ? statusLabel(status.value) : "Unclassified"}
        </span>
        <span className="mono">{manuscript_id?.value ?? "No manuscript ID"}</span>
      </div>
      <p className="journal">{journal?.value ?? "Unknown journal"}</p>
      <dl>
        {deadline && (
          <>
            <dt>Deadline</dt>
            <dd>
              {isoDate(deadline.value)}
              {days !== null && <span className="muted"> · {days >= 0 ? `in ${days} days` : `${-days} days ago`}</span>}
              <span className={`confidence ${confidenceLabel(deadline.confidence)}`}>
                {confidenceLabel(deadline.confidence)}
              </span>
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
            <dd>“{status.original}”</dd>
          </>
        )}
      </dl>
      {uncertainty.length > 0 && (
        <ul className="uncertainty">
          {uncertainty.map((reason) => (
            <li key={reason}>{reason}</li>
          ))}
        </ul>
      )}
    </li>
  );
}
