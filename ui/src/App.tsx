import { Inbox, Library, Lock } from "lucide-react";
import { useCallback, useEffect, useState, type ReactNode } from "react";

import { Manuscripts } from "./components/Manuscripts";
import { ReviewQueue } from "./components/ReviewQueue";
import {
  api,
  inDesktopShell,
  usingMock,
  type AppInfo,
  type ManuscriptSummary,
  type QueueItem,
  type ScanSummary,
} from "./lib/api";

type View = "review" | "manuscripts";

// Deep links: #manuscripts opens the ledger, #manuscripts/<id> also opens that manuscript.
const initialView = (): View =>
  window.location.hash.startsWith("#manuscripts") ? "manuscripts" : "review";
const initialOpenId = (): number | null => {
  const id = Number(window.location.hash.split("/")[1]);
  return Number.isInteger(id) && id > 0 ? id : null;
};

export function App() {
  const live = inDesktopShell() || usingMock;
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [queue, setQueue] = useState<QueueItem[]>([]);
  const [manuscripts, setManuscripts] = useState<ManuscriptSummary[]>([]);
  const [view, setView] = useState<View>(initialView);
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
    if (!live) return;
    api.appInfo().then(
      (i) => {
        setInfo(i);
        if (!i.storage_error) void refresh();
      },
      (e) => setError(String(e)),
    );
  }, [live, refresh]);

  const show = (next: View) => {
    setView(next);
    history.replaceState(null, "", next === "manuscripts" ? "#manuscripts" : "#");
  };

  /** Runs an action, then reloads the review list and manuscripts. */
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

  const storageError = info?.storage_error ?? null;

  return (
    <div className="app">
      <aside className="rail">
        <p className="wordmark">Manuscript Hub</p>
        {live && (
          <nav aria-label="Views">
            <RailLink active={view === "review"} onClick={() => show("review")} count={queue.length}>
              <Inbox aria-hidden="true" size={18} />
              Review
            </RailLink>
            <RailLink
              active={view === "manuscripts"}
              onClick={() => show("manuscripts")}
              count={manuscripts.length}
            >
              <Library aria-hidden="true" size={18} />
              Manuscripts
            </RailLink>
          </nav>
        )}
        <div className="rail-foot">
          {live && (
            <button className="secondary" onClick={scanDemo} disabled={scanning || !!storageError}>
              {scanning ? "Scanning…" : "Scan demo mailbox"}
            </button>
          )}
          <p className="privacy">
            <Lock aria-hidden="true" size={14} />
            Mail is read on this computer and stays here.
          </p>
          {info && (
            <p className="version">
              Version {info.version}, parser {info.parser_version}
              {usingMock && <> (browser preview with sample data)</>}
            </p>
          )}
        </div>
      </aside>

      <main className="page">
        {!live ? (
          <section>
            <h1>Open the desktop app</h1>
            <p className="lede">
              This page is the interface of the desktop app and needs it to run. Start it with{" "}
              <code>npm run tauri dev</code>.
            </p>
          </section>
        ) : (
          <section aria-labelledby="view-title">
            <h1 id="view-title">{view === "review" ? "Review" : "Manuscripts"}</h1>
            <p className="lede">
              {view === "review"
                ? "Messages that look like journal correspondence. Check what was read, then add it to a manuscript or set it aside."
                : "Everything you have confirmed, with the current status and the next deadline."}
            </p>

            {storageError && (
              <p className="notice error" role="alert">
                The local database could not be opened. {storageError}
              </p>
            )}
            <div role="status" className="status-line">
              {scan &&
                `Scanned ${scan.messages} sample messages: ${scan.proposed} new to review, ${scan.already_seen} seen before, ${scan.not_journal_mail} not journal mail.`}
            </div>
            {error && (
              <p className="notice error" role="alert">
                {error}
              </p>
            )}

            {view === "review" ? (
              <ReviewQueue
                items={queue}
                manuscripts={manuscripts}
                scanning={scanning}
                onScan={scanDemo}
                onDecide={(eventId, decision) => act(() => api.reviewEvent(eventId, decision))}
              />
            ) : (
              <Manuscripts
                manuscripts={manuscripts}
                initialOpenId={initialOpenId()}
                onOpenReview={() => show("review")}
              />
            )}
          </section>
        )}
      </main>
    </div>
  );
}

function RailLink(props: {
  active: boolean;
  onClick: () => void;
  count: number;
  children: ReactNode;
}) {
  return (
    <button
      className="rail-link"
      aria-current={props.active ? "page" : undefined}
      onClick={props.onClick}
    >
      <span className="rail-label">{props.children}</span>
      <span className="rail-count" aria-label={`${props.count} items`}>
        {props.count}
      </span>
    </button>
  );
}
