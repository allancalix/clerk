CREATE TABLE IF NOT EXISTS postings (
    id TEXT NOT NULL,
    txn_id TEXT NOT NULL,
    account TEXT NOT NULL,
    amount TEXT NOT NULL,
    currency TEXT NOT NULL,
    status TEXT NOT NULL,

    FOREIGN KEY (txn_id) REFERENCES transactions_new (id),
    PRIMARY KEY (id)
);
