-- ===========================================================================
-- Coding Studio, V1 lot 4 (Review). Applied by `review_schema::migrate` on
-- every open, after schema.sql and the lot 5 schema. Every statement is
-- idempotent. Columns added to existing tables are added in Rust, guarded by
-- a column check.
-- ===========================================================================

-- Concepts the model reported as absent from the codebook, one row per
-- candidate and per answer part. Append-only: a candidate is never edited.
CREATE TABLE IF NOT EXISTS concept_candidates (
    candidate_id  TEXT PRIMARY KEY,            -- CC-0001
    interview_id  TEXT NOT NULL REFERENCES interviews(interview_id),
    run_id        INTEGER NOT NULL REFERENCES ai_runs(run_id),
    part          INTEGER NOT NULL,
    label         TEXT NOT NULL,
    description   TEXT,
    unit_ids      TEXT NOT NULL,               -- JSON array
    evidence      TEXT NOT NULL DEFAULT '[]',  -- JSON array of {unit_id, quote}
    created_at    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_candidates_interview ON concept_candidates(interview_id);

-- The decisions on candidates. Append-only: the current decision is the
-- latest row, the earlier ones stay on record.
CREATE TABLE IF NOT EXISTS concept_decisions (
    decision_id   INTEGER PRIMARY KEY AUTOINCREMENT,
    candidate_id  TEXT NOT NULL REFERENCES concept_candidates(candidate_id),
    action        TEXT NOT NULL,               -- create | map | reject
    code_id       TEXT,                        -- code created or attached to
    rationale     TEXT NOT NULL,
    decided_at    TEXT NOT NULL,
    decided_by    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_concept_decisions ON concept_decisions(candidate_id);

-- Harmonisation worklist (F-38): the units to reread after a merge, a
-- redefinition or a new sub-code. One open item per unit and code.
CREATE TABLE IF NOT EXISTS harmonisation_items (
    item_id       INTEGER PRIMARY KEY AUTOINCREMENT,
    interview_id  TEXT NOT NULL REFERENCES interviews(interview_id),
    unit_id       TEXT NOT NULL,
    code_id       TEXT NOT NULL,               -- stable id of the code concerned
    cause         TEXT NOT NULL,               -- merge | redefine | new_subcode
    detail        TEXT NOT NULL DEFAULT '',
    created_at    TEXT NOT NULL,
    created_by    TEXT NOT NULL,
    resolved_at   TEXT,
    resolved_by   TEXT,
    resolution    TEXT,                        -- confirmed | corrected
    note          TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_harmonisation_open
    ON harmonisation_items(interview_id, unit_id, code_id) WHERE resolved_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_harmonisation_interview ON harmonisation_items(interview_id);

CREATE TRIGGER IF NOT EXISTS concept_candidates_no_update BEFORE UPDATE ON concept_candidates
BEGIN SELECT RAISE(ABORT, 'append-only: concept_candidates'); END;
CREATE TRIGGER IF NOT EXISTS concept_candidates_no_delete BEFORE DELETE ON concept_candidates
BEGIN SELECT RAISE(ABORT, 'append-only: concept_candidates'); END;
CREATE TRIGGER IF NOT EXISTS concept_decisions_no_update BEFORE UPDATE ON concept_decisions
BEGIN SELECT RAISE(ABORT, 'append-only: concept_decisions'); END;
CREATE TRIGGER IF NOT EXISTS concept_decisions_no_delete BEFORE DELETE ON concept_decisions
BEGIN SELECT RAISE(ABORT, 'append-only: concept_decisions'); END;
CREATE TRIGGER IF NOT EXISTS harmonisation_no_delete BEFORE DELETE ON harmonisation_items
BEGIN SELECT RAISE(ABORT, 'append-only: harmonisation_items'); END;
