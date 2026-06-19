CREATE TABLE email_verifications (
    id          UUID PRIMARY KEY,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash  TEXT NOT NULL UNIQUE,
    expires_at  TIMESTAMPTZ NOT NULL DEFAULT now() + interval '24 hours',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX ON email_verifications(user_id);
