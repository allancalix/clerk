# Clerk

A utility for automatically generating Ledger entries from [Plaid API][Plaid]
data.

**Features:**
* Stores and syncs data from Plaid to your local machine.
* Integrates with Plaid Link for linking accounts.
* Provides a powerful scripting language to customer Ledger record output. See
	[Ketos](https://github.com/murarth/ketos) for more information on scripting.

## Example
[![asciicast](https://asciinema.org/a/437024.png)](https://asciinema.org/a/437024)


Without any rules configured, expenses are not categorized and accounts ids are
used in records.

__Default, no scripting rules applied.__
```
2021/09/16 * AUTOMATIC PAYMENT - THANK
    ; TXID: 7Kx6oDoMdMSJ8BQJpQ1lTMK4v8klJ7CgA8D49
    Expenses:Unknown  $2078.50
    NEBoJDJxmxhdoaBdAB7eueRQveKzqyFWao3nD

2021/09/16 * Madison Bicycle Shop
    ; TXID: Plx8JDJa9auRnwERqEjof1QKV5rk9NU7BXqEP
    Expenses:Unknown  $500.00
    NEBoJDJxmxhdoaBdAB7eueRQveKzqyFWao3nD

2021/09/16 * KFC
    ; TXID: jW9jvnvq1qtWbmMWKMZGs6RN3m8rVwF1zvno3
    Expenses:Unknown  $500.00
    NEBoJDJxmxhdoaBdAB7eueRQveKzqyFWao3nD

2021/09/17 * Tectra Inc
    ; TXID: NEBoJDJxmxhdoaBdAB7eueRn65gjqyFWBXRLJ
    Expenses:Unknown  $500.00
    NEBoJDJxmxhdoaBdAB7eueRQveKzqyFWao3nD

2021/09/20 * Uber 072515 SF**POOL**
    ; TXID: KoK1JDJzGzcK1LkKJk9aTpwrW5nZDltVWEdga
    Expenses:Unknown  $6.33
    dLkx161njniBvw6B36LGIWJEvWnmjgfZ9lkD1
```

__Output with [scripting rules](transform.keto)__.
```
2021/09/16 * AUTOMATIC PAYMENT - THANK
    ; TXID: 7Kx6oDoMdMSJ8BQJpQ1lTMK4v8klJ7CgA8D49
    Expenses:Unknown  $2078.50
    Liabilities:Plaid Credit Card

2021/09/16 * Madison Bicycle Shop
    ; TXID: Plx8JDJa9auRnwERqEjof1QKV5rk9NU7BXqEP
    Expenses:Shopping  $500.00
    Liabilities:Plaid Credit Card

2021/09/16 * KFC
    ; TXID: jW9jvnvq1qtWbmMWKMZGs6RN3m8rVwF1zvno3
    Expenses:Food:Restaurant  $500.00
    Liabilities:Plaid Credit Card

2021/09/17 * Tectra Inc
    ; TXID: NEBoJDJxmxhdoaBdAB7eueRn65gjqyFWBXRLJ
    Expenses:Shopping  $500.00
    Liabilities:Plaid Credit Card

2021/09/20 * Uber 072515 SF**POOL**
    ; TXID: KoK1JDJzGzcK1LkKJk9aTpwrW5nZDltVWEdga
    Expenses:Transportation:Rideshare  $6.33
    Assets:Plaid Checking
```

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
clerk requires a [configuration file](ledgersync.toml) and optionally one
or more [keto scripts](transform.keto) for processing transactions into Ledger
entries. The script in this repository highlights some features of scripting
provides such as __categorizing, aliasing, and regex-based matching__.

```sh
# Providing a configuration file explicitly.
clerk -c ledgersync.toml accounts
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
# Initializes a configuration file with interactive shell prompts.
clerk init

clerk link
# Visit http://127.0.0.1:3030/link to link a new account

# List all link items and their current status.
clerk link status

# Delete a link item from account links preventing future queries from retturning
# data for this link. This does not delete transaction of account data.

clerk link delete <ITEM_ID>
```

### Transactions
Commands for interacting with transaction data for all tracked accounts.

```sh
# Prints out Ledger records for each transaction, can be filtered out by date using
# the --begin and --until flags respectively.
clerk transactions print

# Pulls all transaction data for all accounts tracked in the given time range. By
# default, this command pulls the last 2 weeks of transactions. Time range can be
# manipulated using the --begin and --until commands.
clerk transactions sync

# Filtering the first two weeks of September.
clerk transactions sync --begin 2021-09-01 --until 2021-09-14
```

### Accounts
Commands for displaying data about accounts that are currently tracked (i.e.
accounts you have an active access token for).

```sh
# List all tracked accounts.
clerk accounts
# Display the current balance for tracked accounts. This command pulls the latest
# data and tends to be relatively slow.
clerk accounts balance
```

## Caveats
This tool is meant to simplify the maintenance of a personal Ledger record, please
consider where and how your data is stored (please don't run this on a public
machine). There may be - or definitely are - rough edges as pre-release software.

## Roadmap
- [ ] Interop with existing Ledger files to update records for cleared transactions.
- [ ] Expand on storage options (e.g. encryption / remote-storage)
- [ ] Improve ketos interpreter error messaging for debugging scripts.
- [ ] Add caching to balance / accounts lookups

[Plaid]: https://plaid.com/docs/api/ "Plaid Docs"
[Plaid Link]: https://plaid.com/docs/link/ "Plaid Link Documentation"
[Standard Directories]: https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/FileSystemOverview/FileSystemOverview.html#//apple_ref/doc/uid/TP40010672-CH2-SW6

## Troubleshooting
### I'm using `clerk` for the first time and getting data file doesn't exist errors.
If a database file doesn't already exist one won't be created automatically to
prevent saving data in an unexpected place. You can either initialize and provide
your own db file using sqlite or add `?mode=rwc` to your `db_file` path.

```toml
db_file = "./clerk/clerk.db?mode=rwc"
```
