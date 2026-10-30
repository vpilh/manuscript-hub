-- The user's own rules for which senders count as journal mail (FR07).

CREATE TABLE sender_rules (
    -- Lowercase domain; subdomains match too.
    domain      TEXT PRIMARY KEY,
    kind        TEXT NOT NULL CHECK (kind IN ('journal', 'ignore')),
    created_at  INTEGER NOT NULL
);
