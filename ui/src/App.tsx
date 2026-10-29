import { useCallback, useEffect, useState, type ReactNode } from "react";

import { Manuscripts } from "./components/Manuscripts";
import { ReviewQueue } from "./components/ReviewQueue";
import {
  api,
  inDesktopShell,
  type AppInfo,
  type ManuscriptSummary,
  type QueueItem,
  type ScanSummary,
} from "./lib/api";

type Tab = "review" | "manuscripts";

export function App() {
  const desktop = inDesktopShell();
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [queue, setQueue] = useState<QueueItem[]>([]);
  const [manuscripts, setManuscripts] = useState<ManuscriptSummary[]>([]);
  const [tab, setTab] = useState<Tab>("review");
  const [error, setError] = useState<string | null>(null);
  const [scan, setScan] = useState<ScanSummary | null>(null);
  const [scanning, setScanning] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [q, m] = await Promise.all([api.reviewQueue(), api.manuscripts()]);
      setQueue(q);
      setManuscripts(m);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    if (!desktop) return;
    api.appInfo().then((i) => {
      setInfo(i);
      if (!i.storage_error) void refresh();
    }, (e) => setError(String(e)));
  }, [desktop, refresh]);

  /** Runs an action, then reloads queue and manuscripts. */
  const act = useCallback(
    async (action: () => Promise<unknown>) => {
      setError(null);
      try {
        await action();
      } catch (e) {
        setError(String(e));
      }
      await refresh();
    },
    [refresh],
  );

  async function scanDemo() {
    setScanning(true);
    await act(async () => setScan(await api.scanDemoMailbox()));
    setScanning(false);
  }

  if (!desktop) {
    return (
      <main className="shell">
        <Masthead info={null} />
        <p className="notice">
          This is the browser preview. Run <code>npm run tauri dev</code> to start the desktop app.
        </p>
      </main>
    );
  }

  return (
    <main className="shell">
      <Masthead info={info} />

      {info?.storage_error && (
        <p className="notice error">
          The local database could not be opened: {info.storage_error}
        </p>
      )}

      <div className="toolbar">
        <nav className="tabs" aria-label="Views">
          <TabButton active={tab === "review"} onClick={() => setTab("review")}>
            Review <span className="count">{queue.length}</span>
          </TabButton>
          <TabButton active={tab === "manuscripts"} onClick={() => setTab("manuscripts")}>
            Manuscripts <span className="count">{manuscripts.length}</span>
          </TabButton>
        </nav>
        <button onClick={scanDemo} disabled={scanning || !!info?.storage_error}>
          {scanning ? "Scanning…" : "Scan demo mailbox"}
        </button>
      </div>

      {scan && (
        <p className="muted small">
          Scanned {scan.messages} synthetic messages: {scan.proposed} new for review,{" "}
          {scan.already_seen} already seen, {scan.not_journal_mail} not journal mail.
        </p>
      )}
      {error && <p className="notice error">{error}</p>}

      {tab === "review" ? (
        <ReviewQueue
          items={queue}
          manuscripts={manuscripts}
          onDecide={(eventId, decision) => act(() => api.reviewEvent(eventId, decision))}
        />
      ) : (
        <Manuscripts manuscripts={manuscripts} />
      )}
    </main>
  );
}

function Masthead({ info }: { info: AppInfo | null }) {
  return (
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
  );
}

function TabButton(props: { active: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      className={`tab ${props.active ? "active" : ""}`}
      aria-pressed={props.active}
      onClick={props.onClick}
    >
      {props.children}
    </button>
  );
}
