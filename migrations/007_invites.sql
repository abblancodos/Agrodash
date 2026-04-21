-- migrations/007_invites.sql

CREATE TABLE IF NOT EXISTS invites (
    code       TEXT        PRIMARY KEY,       -- 10 chars ASCII random
    created_by UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    email_hint TEXT,                          -- opcional, para quién es
    expires_at TIMESTAMPTZ NOT NULL,
    used_at    TIMESTAMPTZ,
    used_by    UUID        REFERENCES users(id)
);

CREATE INDEX IF NOT EXISTS idx_invites_expires ON invites (expires_at);
CREATE INDEX IF NOT EXISTS idx_invites_used    ON invites (used_at) WHERE used_at IS NULL;
