CREATE TABLE room (
  id TEXT PRIMARY KEY,
  state JSON,
  last_active DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
