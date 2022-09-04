# Clerk
A utility for automatically syncing transactions from [Plaid API][Plaid].

**Features:**
* Sync transaction data from Plaid to your local machine
* Integration with [Plaid Link][Plaid Link]
* Keeps local data consistent with Plaid data as transactions are modified

## Motivation
Having a consistent and accurate source of transaction data makes tasks like
budgeting, accounting, and taxes easier. I use this transaction stream to
automatically generate [beancount](https://beancount.github.io/) entries for Plain
text accounting.

At the end of the day the data is yours and you can query the database using sqlite
directly for building UIs, account monitoring, or all sorts of purposes. For some
examples check out:
* [clerk beancount importer](https://github.com/allancalix/clerk-importer)
* [clerk account alerting example](https://github.com/allancalix/clerk-importer)

## Installation

### Building from source
```sh
# Binary found in ./target/release/clerk
cargo build --release
```

### Prebuilt Binaries
Prebuilt binaries for Linux and MacOS (pre-M1) can be found for the latest
[release](https://github.com/allancalix/clerk/releases).

## Usage

### Configuration
clerk requires a [configuration file](clerk.toml).

```sh
# Providing a configuration file explicitly.
clerk -c clerk.toml accounts
```

If no configuration is explicitly given, clerk will search for a file called
`config.toml` in directories based on the [XDG user directory spec](https://www.freedesktop.org/wiki/Software/xdg-user-dirs/)
on Linux and the [Standard Directories][] on MacOS.

Data is stored transparently as a single Json file, it's location is based on the
same pair of specifications. __Be mindful of where you store this file as it
contains transaction history for linked accounts__.

### Link
Link is used to create an access token for a set of credentials linked to an
institution. This is done by serving [Plaid Link][] on your local machine to
perform authentication.

```sh
# Link a new account to Plaid.
clerk link
# Refresh a linked accounts status, periodically required for some accounts
clerk link --update <LINK_ID>

# List all link items and their current status.
clerk link status

# Delete a link item from account links preventing future queries from retturning
# data for this link. This does not delete transaction of account data.

clerk link delete <ITEM_ID>
```

### Transactions
Commands for interacting with transaction data for all tracked accounts.

```sh
# Syncs transactions for all tracked accounts with upstream data. The first sync
# may take a few seconds (it pulls up to 24 months of transaction history). Each
# consecutive sync will only pull the latest data and should take less time.
clerk txn sync
```

### Accounts
Commands for displaying data about accounts that are currently tracked (i.e.
accounts you have an active access token for).

```sh
# List all tracked accounts.
clerk account

# Display the current balance for tracked accounts. This command pulls the latest
# data and tends to be relatively slow.
clerk account balances
```

## Caveats
This tool is meant to simplify the maintenance of a personal plaintext finance records,
please consider where and how your data is stored (please don't run this on a public
machine). Note that data is stored in a local sqlite database as plain text and
makes no effort to encrypt or otherwise obfuscate your data.

This single database file can be encrypted for additional security using a file
encryption tool like [age](https://github.com/FiloSottile/age).

## Roadmap
- [ ] Automatic transaction deduping between linked accounts
- [ ] Expand on storage options (e.g. encryption / remote-storage)

## Troubleshooting
### I'm using `clerk` for the first time and getting data file doesn't exist errors.
If a database file doesn't already exist one won't be created automatically to
prevent saving data in an unexpected place. You can either initialize and provide
your own db file using sqlite or add `?mode=rwc` to your `db_file` path.

```toml
db_file = "./clerk/clerk.db?mode=rwc"
```

[Plaid]: https://plaid.com/docs/api/ "Plaid Docs"
[Plaid Link]: https://plaid.com/docs/link/ "Plaid Link Documentation"
[Standard Directories]: https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/FileSystemOverview/FileSystemOverview.html#//apple_ref/doc/uid/TP40010672-CH2-SW6
