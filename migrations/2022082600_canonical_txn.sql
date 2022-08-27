CREATE TABLE IF NOT EXISTS transactions_new (
    id TEXT NOT NULL,
    date TEXT NOT NULL,
    narration TEXT NOT NULL,
    payee TEXT,
    source TEXT,
    status TEXT,

    PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS int_transactions_links (
  item_id TEXT NOT NULL,
  txn_id TEXT NOT NULL,
  plaid_txn_id TEXT NOT NULL,

  FOREIGN KEY (item_id) REFERENCES plaid_links (item_id),
  FOREIGN KEY (txn_id) REFERENCES transactions_new (id)
);

CREATE TABLE IF NOT EXISTS tags (
  id TEXT NOT NULL,
  txn_id TEXT NOT NULL,
  value TEXT NOT NULL,

  FOREIGN KEY (txn_id) REFERENCES transactions_new (id)
);
