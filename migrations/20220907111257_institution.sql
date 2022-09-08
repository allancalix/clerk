ALTER TABLE plaid_links
  ADD column institution TEXT;

CREATE TABLE IF NOT EXISTS institutions (
  id TEXT NOT NULL,
  name TEXT NOT NULL,

  PRIMARY KEY (id)
);

