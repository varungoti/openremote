CREATE TABLE devices (
  id UUID PRIMARY KEY,
  display_id CHAR(9) UNIQUE NOT NULL,
  name TEXT NOT NULL,
  platform TEXT NOT NULL,
  public_key BYTEA NOT NULL,
  device_token_hash BYTEA NOT NULL,
  owner_user_id UUID,
  unattended_enabled BOOLEAN NOT NULL DEFAULT FALSE,
  online BOOLEAN NOT NULL DEFAULT FALSE,
  last_seen_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE trusted_controllers (
  id UUID PRIMARY KEY,
  host_device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
  controller_public_key BYTEA NOT NULL,
  label TEXT NOT NULL,
  permissions JSONB NOT NULL,
  revoked_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE(host_device_id, controller_public_key)
);

CREATE TABLE remote_sessions (
  id UUID PRIMARY KEY,
  host_device_id UUID NOT NULL REFERENCES devices(id),
  controller_public_key BYTEA NOT NULL,
  status TEXT NOT NULL,
  permissions JSONB NOT NULL,
  transport TEXT,
  relay_region TEXT,
  started_at TIMESTAMPTZ,
  ended_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE audit_events (
  id BIGSERIAL PRIMARY KEY,
  device_id UUID REFERENCES devices(id),
  session_id UUID REFERENCES remote_sessions(id),
  event_type TEXT NOT NULL,
  actor_key_fingerprint TEXT,
  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX audit_events_device_time ON audit_events(device_id, created_at DESC);
CREATE INDEX remote_sessions_host_time ON remote_sessions(host_device_id, created_at DESC);
