import { useId, useState, type FormEvent } from "react";

import type { SenderRule, SenderRuleKind } from "../lib/api";

const KIND_LABELS: Record<SenderRuleKind, string> = {
  journal: "Always journal mail",
  ignore: "Never journal mail",
};

/** The user's own rules for which senders count as journal mail. */
export function SenderRules(props: {
  rules: SenderRule[];
  onAdd: (domain: string, kind: SenderRuleKind) => Promise<void>;
  onRemove: (domain: string) => Promise<void>;
}) {
  const [domain, setDomain] = useState("");
  const [kind, setKind] = useState<SenderRuleKind>("journal");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const titleId = useId();

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await props.onAdd(domain, kind);
      setDomain("");
    } catch (err) {
      setError(String(err));
    }
    setBusy(false);
  }

  return (
    <section className="rules" aria-labelledby={titleId}>
      <h2 id={titleId}>Sender rules</h2>
      <p className="hint">
        Mark a small journal's own domain as journal mail, or stop mail from a sender from being
        suggested. Rules cover subdomains too and apply from the next check.
      </p>

      {props.rules.length > 0 && (
        <ul className="rule-list">
          {props.rules.map((rule) => (
            <li key={rule.domain}>
              <span className="figures">{rule.domain}</span>
              <span className="since">{KIND_LABELS[rule.kind]}</span>
              <button className="link" onClick={() => void props.onRemove(rule.domain)}>
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}

      <form className="field-row" onSubmit={submit}>
        <label className="field grow">
          <span>Sender domain</span>
          <input
            value={domain}
            onChange={(e) => setDomain(e.target.value)}
            placeholder="journal.example.org"
            spellCheck={false}
            autoComplete="off"
          />
        </label>
        <label className="field">
          <span>Treat as</span>
          <select value={kind} onChange={(e) => setKind(e.target.value as SenderRuleKind)}>
            {(Object.keys(KIND_LABELS) as SenderRuleKind[]).map((k) => (
              <option key={k} value={k}>
                {KIND_LABELS[k]}
              </option>
            ))}
          </select>
        </label>
        <div className="decide rule-add">
          <button type="submit" className="secondary" disabled={busy || !domain.trim()}>
            Add rule
          </button>
        </div>
      </form>
      {error && (
        <p className="notice error" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
