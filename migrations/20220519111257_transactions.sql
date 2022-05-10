CREATE TABLE IF NOT EXISTS transactions (
    item_id text NOT NULL,
    transaction_id text NOT NULL,
    payload BLOB NOT NULL,

    primary key (item_id, transaction_id)
);