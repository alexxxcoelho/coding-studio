-- Persons, organisations and groups (the people interviewed). Applied on
-- every open by `persons::migrate`; every statement is idempotent.

CREATE TABLE IF NOT EXISTS person_counters (
    name   TEXT PRIMARY KEY,
    value  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS persons (
    person_id         TEXT PRIMARY KEY,
    kind              TEXT NOT NULL DEFAULT 'person',
    name              TEXT NOT NULL,
    organisation      TEXT NOT NULL DEFAULT '',
    person_function   TEXT NOT NULL DEFAULT '',
    email             TEXT NOT NULL DEFAULT '',
    phone             TEXT NOT NULL DEFAULT '',
    notes             TEXT NOT NULL DEFAULT '',
    consent_status    TEXT NOT NULL DEFAULT '',
    consent_date      TEXT NOT NULL DEFAULT '',
    consent_document  TEXT NOT NULL DEFAULT '',
    anonymise         INTEGER NOT NULL DEFAULT 1,
    pseudonym         TEXT NOT NULL,
    aliases           TEXT NOT NULL DEFAULT '[]',
    attributes        TEXT NOT NULL DEFAULT '{}',
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS person_groups (
    group_id     TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    parent_id    TEXT REFERENCES person_groups(group_id),
    description  TEXT NOT NULL DEFAULT '',
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_person_groups_name
    ON person_groups(COALESCE(parent_id, ''), lower(name));

CREATE TABLE IF NOT EXISTS person_group_members (
    person_id  TEXT NOT NULL REFERENCES persons(person_id) ON DELETE CASCADE,
    group_id   TEXT NOT NULL REFERENCES person_groups(group_id),
    PRIMARY KEY (person_id, group_id)
);

CREATE TABLE IF NOT EXISTS person_interviews (
    person_id     TEXT NOT NULL REFERENCES persons(person_id) ON DELETE CASCADE,
    interview_id  TEXT NOT NULL REFERENCES interviews(interview_id),
    role          TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    PRIMARY KEY (person_id, interview_id)
);
CREATE INDEX IF NOT EXISTS idx_person_interviews_iv ON person_interviews(interview_id);
