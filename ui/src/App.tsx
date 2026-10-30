import { Inbox, Library, Lock, Mail } from "lucide-react";
import { useCallback, useEffect, useState, type ReactNode } from "react";

import { Mailboxes } from "./components/Mailboxes";
import { scanSentence } from "./lib/format";
import { Manuscripts } from "./components/Manuscripts";
import { ReviewQueue } from "./components/ReviewQueue";
import {
  api,
  inDesktopShell,
  usingMock,
  type AppInfo,
  type MailAccount,
  type ManuscriptSummary,
  type SenderRule,
  type QueueItem,
  type ScanSummary,
} from "./lib/api";

type View = "review" | "manuscripts" | "mailboxes";

const TITLES: Record<View, [string, string]> = {
  review: [
    "Review",
    "Messages that look like journal correspondence. Check what was read, then add it to a manuscript or set it aside.",
  ],
  manuscripts: [
    "Manuscripts",
    "Everything you have confirmed, with the current status and the next deadline.",
  ],
  mailboxes: [
    "Mailboxes",
    "The mail accounts this app reads. Journal mail they contain shows up for review.",
  ],
};

// Deep links: #manuscripts opens the ledger, #manuscripts/<id> also opens that
// manuscript, #mailboxes opens the accounts.
const initialView = (): View => {
  const hash = window.location.hash;
  if (hash.startsWith("#manuscripts")) return "manuscripts";
  if (hash.startsWith("#mailboxes")) return "mailboxes";
  return "review";
};
const initialOpenId = (): number | null => {
  const id = Number(window.location.hash.split("/")[1]);
  return Number.isInteger(id) && id > 0 ? id : null;
};

export function App() {
  const live = inDesktopShell() || usingMock;
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [queue, setQueue] = useState<QueueItem[]>([]);
  const [manuscripts, setManuscripts] = useState<ManuscriptSummary[]>([]);
  const [accounts, setAccounts] = useState<MailAccount[]>([]);
  const [senderRules, setSenderRules] = useState<SenderRule[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [paused, setPaused] = useState(false);
  const [checking, setChecking] = useState<ReadonlySet<number>>(new Set());
  const [checkResults, setCheckResults] = useState<Record<number, string>>({});
  const [view, setView] = useState<View>(initialView);
  const [error, setError] = useState<string | null>(null);
  const [scan, setScan] = useState<ScanSummary | null>(null);
  const [scanning, setScanning] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [q, m, a, r] = await Promise.all([
        api.reviewQueue(),
        api.manuscripts(),
        api.mailAccounts(),
        api.senderRules(),
      ]);
      setQueue(q);
      setManuscripts(m);
      setAccounts(a);
      setSenderRules(r);
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

  useEffect(() => {
    if (!live) return;
    void api.checksPaused().then(setPaused, () => {});
    const unlisten = api.onChecksPaused(setPaused);
    return () => void unlisten.then((stop) => stop());
  }, [live]);

  // Scheduled and manual checks report here; new proposals show up at once.
  useEffect(() => {
    if (!live) return;
    const unlisten = api.onMailCheck((event) => {
      setChecking((current) => {
        const next = new Set(current);
        if (event.running) next.add(event.account_id);
        else next.delete(event.account_id);
        return next;
      });
      if (!event.running) {
        if (event.summary) {
          const sentence = scanSentence(event.summary);
          setCheckResults((r) => ({ ...r, [event.account_id]: sentence }));
        }
        void refresh();
      }
    });
    return () => void unlisten.then((stop) => stop());
  }, [live, refresh]);

  const show = (next: View) => {
    setView(next);
    history.replaceState(null, "", next === "review" ? "#" : `#${next}`);
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
    setNotice(null);
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
            <RailLink
              active={view === "review"}
              onClick={() => show("review")}
              count={queue.length}
            >
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
            <RailLink
              active={view === "mailboxes"}
              onClick={() => show("mailboxes")}
              count={accounts.length}
            >
              <Mail aria-hidden="true" size={18} />
              Mailboxes
            </RailLink>
          </nav>
        )}
        <div className="rail-foot">
          {live && (
            <button className="secondary" onClick={scanDemo} disabled={scanning || !!storageError}>
              {scanning ? "Scanning…" : "Scan demo mailbox"}
            </button>
          )}
          {live && accounts.length > 0 && (
            <p className="checks-state" role="status">
              {paused ? "Automatic checks are paused." : "Mailboxes are checked automatically."}{" "}
              <button className="link" onClick={() => void api.setChecksPaused(!paused)}>
                {paused ? "Resume" : "Pause"}
              </button>
            </p>
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
            <h1 id="view-title">{TITLES[view][0]}</h1>
            <p className="lede">{TITLES[view][1]}</p>

            {storageError && (
              <p className="notice error" role="alert">
                The local database could not be opened. {storageError}
              </p>
            )}
            <div role="status" className="status-line">
              {scan &&
                `Scanned ${scan.messages} sample messages: ${scan.proposed} new to review, ${scan.already_seen} seen before, ${scan.not_journal_mail} not journal mail.`}
              {!scan && notice}
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
                hasMailboxes={accounts.length > 0}
                onScan={scanDemo}
                onOpenMailboxes={() => show("mailboxes")}
                onBlockSender={(domain) =>
                  act(async () => {
                    const dismissed = await api.setSenderRule(domain, "ignore");
                    setScan(null);
                    setNotice(
                      `Mail from ${domain} won't be suggested again` +
                        (dismissed > 0
                          ? `; ${dismissed} waiting ${dismissed === 1 ? "item was" : "items were"} set aside.`
                          : ".") +
                        " You can undo this under Mailboxes, Sender rules.",
                    );
                  })
                }
                onDecide={(eventId, decision) => act(() => api.reviewEvent(eventId, decision))}
              />
            ) : view === "mailboxes" ? (
              <Mailboxes
                accounts={accounts}
                checking={checking}
                results={checkResults}
                onConnect={async (draft) => {
                  const account = await api.connectMailAccount(draft);
                  await refresh();
                  return account;
                }}
                onSync={async (accountId) => {
                  try {
                    const summary = await api.syncMailAccount(accountId);
                    setCheckResults((r) => ({ ...r, [accountId]: scanSentence(summary) }));
                  } catch {
                    // The error is stored on the account and shown in its row.
                  } finally {
                    await refresh();
                  }
                }}
                onSetInterval={(accountId, minutes) =>
                  act(() => api.setCheckInterval(accountId, minutes))
                }
                onDisconnect={(accountId) => act(() => api.disconnectMailAccount(accountId))}
                senderRules={senderRules}
                onAddRule={async (domain, kind) => {
                  // Errors (an invalid domain) are shown next to the form.
                  await api.setSenderRule(domain, kind);
                  await refresh();
                }}
                onRemoveRule={(domain) => act(() => api.removeSenderRule(domain))}
              />
            ) : (
              <Manuscripts
                manuscripts={manuscripts}
                initialOpenId={initialOpenId()}
                onOpenReview={() => show("review")}
                onMerge={(fromId, intoId) => act(() => api.mergeManuscripts(fromId, intoId))}
                onAmend={(eventId, edits) => act(() => api.amendEvent(eventId, edits))}
                onRemove={(eventId) => act(() => api.removeEvent(eventId))}
                onAddManual={async (target, update) => {
                  const id = await api.addManualUpdate(target, update);
                  await refresh();
                  return id;
                }}
                onPaste={async (text) => {
                  const outcome = await api.importPastedMail(text);
                  await refresh();
                  return outcome;
                }}
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
