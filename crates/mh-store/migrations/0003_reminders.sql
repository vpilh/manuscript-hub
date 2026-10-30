-- Deadline reminders already shown, so each is sent once (FR14).

CREATE TABLE reminders_sent (
    manuscript_id  INTEGER NOT NULL REFERENCES manuscripts (id) ON DELETE CASCADE,
    deadline       TEXT NOT NULL,
    stage          TEXT NOT NULL CHECK (stage IN ('week', 'day', 'overdue')),
    sent_at        INTEGER NOT NULL,
    PRIMARY KEY (manuscript_id, deadline, stage)
);
