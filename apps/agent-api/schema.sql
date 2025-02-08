CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE accounts
(
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    clerk_id TEXT NOT NULL UNIQUE,
    public_token TEXT NOT NULL UNIQUE,
    secret_key TEXT NOT NULL UNIQUE,
    allowed_origins TEXT[],
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TYPE lead_status_enum AS ENUM ('non_lead', 'lead', 'converted');

CREATE TABLE visitors
(
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID REFERENCES accounts(id) ON DELETE CASCADE,
    fingerprint TEXT NOT NULL UNIQUE,
    hashed_email TEXT UNIQUE,
    country TEXT,
    lead_status lead_status_enum DEFAULT 'non_lead' NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE events
(
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    visitor_id UUID REFERENCES visitors(id) ON DELETE CASCADE,
    data JSONB NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE OR REPLACE FUNCTION update_timestamp()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = CURRENT_TIMESTAMP;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_update_accounts
BEFORE UPDATE ON accounts
FOR EACH ROW EXECUTE FUNCTION update_timestamp();

CREATE TRIGGER trigger_update_visitors
BEFORE UPDATE ON visitors
FOR EACH ROW EXECUTE FUNCTION update_timestamp();

CREATE TRIGGER trigger_update_events
BEFORE UPDATE ON events
FOR EACH ROW EXECUTE FUNCTION update_timestamp();

CREATE INDEX idx_accounts_id ON accounts USING btree (id);
CREATE INDEX idx_visitors_id ON visitors USING btree (id);
CREATE INDEX idx_events_visitor_id ON events (visitor_id);

-- Thread table
-- CREATE TABLE thread (
--     thread_id UUID PRIMARY KEY,
--     user_id INT REFERENCES users(id),
--     status TEXT NOT NULL DEFAULT 'active',  -- Status of the thread (active, paused, completed)
--     metadata JSONB,                         -- Additional metadata for the thread
--     created_at TIMESTAMP DEFAULT NOW(),
--     updated_at TIMESTAMP DEFAULT NOW()
-- );