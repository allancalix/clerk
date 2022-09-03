CREATE TABLE IF NOT EXISTS plaid_links (
    id TEXT NOT NULL,
    alias TEXT,
    access_token TEXT,
    link_state TEXT CHECK( link_state IN ('ACTIVE','REQUIRES_VERIFICATION') ) NOT NULL DEFAULT 'ACTIVE',
    sync_cursor TEXT,

    PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS accounts (
  id TEXT NOT NULL,
  item_id TEXT NOT NULL,
  name TEXT NOT NULL,
  type TEXT,

  FOREIGN KEY (item_id) REFERENCES plaid_links (id),
  PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS transactions (
    id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    source TEXT,

    FOREIGN KEY (account_id) REFERENCES accounts (id),
    PRIMARY KEY (id)
);
