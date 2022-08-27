CREATE TABLE IF NOT EXISTS transactions (
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
  FOREIGN KEY (txn_id) REFERENCES transactions (id)
);

CREATE TABLE IF NOT EXISTS tags (
  id TEXT NOT NULL,
  txn_id TEXT NOT NULL,
  value TEXT NOT NULL,

  FOREIGN KEY (txn_id) REFERENCES transactions (id)
);

CREATE TABLE IF NOT EXISTS postings (
    id TEXT NOT NULL,
    txn_id TEXT NOT NULL,
    account TEXT NOT NULL,
    amount TEXT NOT NULL,
    currency TEXT NOT NULL,
    status TEXT NOT NULL,

    FOREIGN KEY (txn_id) REFERENCES transactions (id),
    PRIMARY KEY (id)
);
