import { TriangleAlert } from "lucide-react";
import { useId, useState, type FormEvent } from "react";

import type { MailAccount, MailProvider, NewMailAccount, ScanSummary, Security } from "../lib/api";
import { checkedAgo, scanSentence } from "../lib/format";

interface Props {
  accounts: MailAccount[];
  onConnect: (account: NewMailAccount) => Promise<MailAccount>;
  onSync: (accountId: number) => Promise<ScanSummary>;
  onDisconnect: (accountId: number) => Promise<void>;
}

export function Mailboxes({ accounts, onConnect, onSync, onDisconnect }: Props) {
  // Last check result per account, shown next to it.
  const [results, setResults] = useState<Record<number, string>>({});
  const [syncing, setSyncing] = useState<number | null>(null);

  async function sync(accountId: number) {
    setSyncing(accountId);
    try {
      const summary = await onSync(accountId);
      setResults((r) => ({ ...r, [accountId]: scanSentence(summary) }));
    } catch {
      // The error is stored on the account and shown in its row.
      setResults((r) => ({ ...r, [accountId]: "" }));
    }
    setSyncing(null);
  }

  return (
    <>
      {accounts.length > 0 && (
        <ul className="ledger accounts" aria-label="Connected mailboxes">
          {accounts.map((account) => (
            <AccountRow
              key={account.id}
              account={account}
              result={results[account.id] ?? null}
              syncing={syncing === account.id}
              busy={syncing !== null}
              onSync={() => sync(account.id)}
              onDisconnect={() => onDisconnect(account.id)}
            />
          ))}
        </ul>
      )}
      <ConnectForm
        first={accounts.length === 0}
        onConnect={async (draft) => {
          const account = await onConnect(draft);
          void sync(account.id);
        }}
      />
    </>
  );
}

function AccountRow(props: {
  account: MailAccount;
  result: string | null;
  syncing: boolean;
  busy: boolean;
  onSync: () => void;
  onDisconnect: () => Promise<void>;
}) {
  const { account, result, syncing, busy } = props;
  const [confirming, setConfirming] = useState(false);
  const { settings } = account;
  const gmail = account.provider === "gmail";

  return (
    <li className="ledger-row account">
      <div className="account-main">
        <div className="account-name">
          <span className="ledger-journal">{account.alias}</span>
          <span className="since">
            {gmail ? settings.username : `${settings.username} on ${settings.host}`}
          </span>
          <span className="since">Reads {settings.folders.join(", ")}</span>
        </div>
        <div className="account-state" role="status">
          {syncing ? (
            <span>Checking for new mail… The first check can take a few minutes.</span>
          ) : account.last_error ? null : account.last_sync_at ? (
            <span>Checked {checkedAgo(account.last_sync_at, Date.now() / 1000)}</span>
          ) : (
            <span className="since">Not checked yet</span>
          )}
          {!syncing && result && <span className="since">{result}</span>}
        </div>
      </div>

      {account.last_error && !syncing && (
        <p className="account-error" role="alert">
          <TriangleAlert aria-hidden="true" size={16} />
          {account.last_error}
        </p>
      )}

      {confirming ? (
        <div className="decide">
          <span className="confirm-text">
            Disconnect {account.alias}? The password is removed from this computer. Manuscripts
            you confirmed stay.
          </span>
          <button className="secondary" onClick={() => void props.onDisconnect()}>
            Disconnect
          </button>
          <button className="quiet" onClick={() => setConfirming(false)}>
            Keep it
          </button>
        </div>
      ) : (
        <div className="decide">
          <button onClick={props.onSync} disabled={busy}>
            {syncing ? "Checking…" : "Check for new mail"}
          </button>
          <button className="quiet" onClick={() => setConfirming(true)} disabled={syncing}>
            Disconnect
          </button>
        </div>
      )}
    </li>
  );
}

const LOOK_BACK = [
  { days: 90, label: "3 months" },
  { days: 182, label: "6 months" },
  { days: 365, label: "1 year" },
  { days: 730, label: "2 years" },
];

function ConnectForm(props: { first: boolean; onConnect: (draft: NewMailAccount) => Promise<void> }) {
  const [provider, setProvider] = useState<MailProvider>("gmail");
  const [alias, setAlias] = useState("");
  const [host, setHost] = useState("");
  const [port, setPort] = useState("993");
  const [security, setSecurity] = useState<Security>("tls");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [folder, setFolder] = useState("INBOX");
  const [backfill, setBackfill] = useState(182);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const titleId = useId();
  const passwordHelp = useId();
  const gmail = provider === "gmail";

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await props.onConnect({
        provider,
        alias: alias.trim() || (gmail ? "Gmail" : "University mail"),
        settings: {
          host: gmail ? "imap.gmail.com" : host.trim(),
          port: gmail ? 993 : Number(port),
          security: gmail ? "tls" : security,
          username: username.trim(),
          folders: gmail ? [] : [folder.trim() || "INBOX"],
          backfill_days: backfill,
        },
        password,
      });
      setPassword("");
      setUsername("");
      setAlias("");
    } catch (err) {
      setError(String(err));
    }
    setBusy(false);
  }

  const portValid = /^\d{1,5}$/.test(port) && Number(port) > 0 && Number(port) < 65536;
  const ready = username.trim() && password && (gmail || (host.trim() && portValid));

  return (
    <form className="connect" aria-labelledby={titleId} onSubmit={submit}>
      <h2 id={titleId}>{props.first ? "Connect a mailbox" : "Connect another mailbox"}</h2>
      <p className="hint">
        Mail is only read. Nothing is marked as read, moved or deleted, and the password is kept
        in this computer's keychain.
      </p>

      <fieldset className="choice">
        <legend>Mailbox</legend>
        <label>
          <input
            type="radio"
            name="provider"
            checked={gmail}
            onChange={() => setProvider("gmail")}
          />
          Gmail
        </label>
        <label>
          <input
            type="radio"
            name="provider"
            checked={!gmail}
            onChange={() => setProvider("groupwise_imap")}
          />
          University or other IMAP mail
        </label>
      </fieldset>

      {!gmail && (
        <>
          <div className="field-row">
            <label className="field grow">
              <span>Server</span>
              <input
                value={host}
                onChange={(e) => setHost(e.target.value)}
                placeholder="imap.university.example"
                autoComplete="off"
                spellCheck={false}
              />
            </label>
            <label className="field narrow">
              <span>Port</span>
              <input
                value={port}
                inputMode="numeric"
                aria-invalid={!portValid}
                onChange={(e) => setPort(e.target.value)}
              />
            </label>
          </div>
          <label className="field">
            <span>Encryption</span>
            <select
              value={security}
              onChange={(e) => {
                const next = e.target.value as Security;
                setSecurity(next);
                if (port === "993" || port === "143") setPort(next === "tls" ? "993" : "143");
              }}
            >
              <option value="tls">TLS (usually port 993)</option>
              <option value="start_tls">STARTTLS (usually port 143)</option>
            </select>
          </label>
          <p className="hint">
            If your university only allows mail access from its network, connect to the VPN
            first.
          </p>
        </>
      )}

      <label className="field">
        <span>{gmail ? "Gmail address" : "User name"}</span>
        <input
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          autoComplete="off"
          spellCheck={false}
          type={gmail ? "email" : "text"}
        />
      </label>

      <label className="field">
        <span>{gmail ? "App password" : "Password"}</span>
        <input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete="new-password"
          aria-describedby={gmail ? passwordHelp : undefined}
        />
      </label>
      {gmail && (
        <p className="hint" id={passwordHelp}>
          Your Google password won't work here. Create an app password at
          myaccount.google.com/apppasswords; it needs 2-Step Verification turned on.
        </p>
      )}

      {!gmail && (
        <label className="field">
          <span>Folder</span>
          <input
            value={folder}
            onChange={(e) => setFolder(e.target.value)}
            spellCheck={false}
            autoComplete="off"
          />
        </label>
      )}

      <div className="field-row">
        <label className="field grow">
          <span>Name in this app</span>
          <input
            value={alias}
            onChange={(e) => setAlias(e.target.value)}
            placeholder={gmail ? "Gmail" : "University mail"}
          />
        </label>
        <label className="field">
          <span>First check reads</span>
          <select value={backfill} onChange={(e) => setBackfill(Number(e.target.value))}>
            {LOOK_BACK.map((option) => (
              <option key={option.days} value={option.days}>
                the last {option.label}
              </option>
            ))}
          </select>
        </label>
      </div>

      {error && (
        <p className="notice error" role="alert">
          {error}
        </p>
      )}

      <div className="decide">
        <button type="submit" disabled={busy || !ready}>
          {busy ? "Checking the login…" : gmail ? "Connect Gmail" : "Connect mailbox"}
        </button>
      </div>
    </form>
  );
}
