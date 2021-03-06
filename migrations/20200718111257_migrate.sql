CREATE TABLE IF NOT EXISTS plaid_links (
    id integer primary key autoincrement,
    item_id text NOT NULL,
    alias text,
    access_token text,
    link_state text CHECK( link_state IN ('ACTIVE','REQUIRES_VERIFICATION') ) NOT NULL DEFAULT 'ACTIVE',
    environment text CHECK( environment IN ('DEVELOPMENT','SANDBOX','PRODUCTION') )
);
