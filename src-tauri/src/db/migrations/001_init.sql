-- SecProof schema. All timestamps in milliseconds (INTEGER). Single migration (merged).

CREATE TABLE IF NOT EXISTS chats (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  mode TEXT NOT NULL DEFAULT 'recon' CHECK (mode IN ('auto', 'recon', 'triage', 'validation', 'assessment')),
  battle_map TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tool_invocations (
  id TEXT PRIMARY KEY,
  chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  tool_name TEXT NOT NULL,
  tool_source TEXT NOT NULL DEFAULT 'local' CHECK (tool_source IN ('local', 'mcp')),
  input_params TEXT NOT NULL,
  target TEXT NOT NULL DEFAULT '',
  raw_output TEXT,
  exit_code INTEGER,
  duration_ms INTEGER,
  approval_id TEXT,
  status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'complete', 'denied', 'failed')),
  phase_name TEXT,
  risk_category TEXT,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
  id TEXT PRIMARY KEY,
  chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'tool')),
  content TEXT NOT NULL,
  tool_invocation_id TEXT REFERENCES tool_invocations(id) ON DELETE SET NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS approvals (
  id TEXT PRIMARY KEY,
  tool_invocation_id TEXT NOT NULL REFERENCES tool_invocations(id) ON DELETE CASCADE,
  decision TEXT NOT NULL CHECK (decision IN ('approved', 'denied', 'dry_run')),
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS findings (
  id TEXT PRIMARY KEY,
  chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  tool_invocation_id TEXT REFERENCES tool_invocations(id) ON DELETE SET NULL,
  title TEXT NOT NULL,
  severity TEXT NOT NULL CHECK (severity IN ('critical', 'high', 'medium', 'low', 'info')),
  description TEXT NOT NULL,
  mitre_ref TEXT,
  owasp_ref TEXT,
  cwe_ref TEXT,
  recommended_action TEXT,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS file_attachments (
  id TEXT PRIMARY KEY,
  chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  filename TEXT NOT NULL,
  file_type TEXT NOT NULL,
  file_path TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  sha256 TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS audit_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  timestamp INTEGER NOT NULL,
  action_type TEXT NOT NULL,
  object_id TEXT NOT NULL,
  summary TEXT NOT NULL,
  metadata TEXT,
  entry_hash TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_chat_id ON messages(chat_id);
CREATE INDEX IF NOT EXISTS idx_tool_invocations_chat_id ON tool_invocations(chat_id);
CREATE INDEX IF NOT EXISTS idx_findings_chat_id ON findings(chat_id);
CREATE INDEX IF NOT EXISTS idx_file_attachments_chat_id ON file_attachments(chat_id);
