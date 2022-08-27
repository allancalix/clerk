# Clerk
A utility for automatically generating beancount entries from [Plaid API][Plaid]
data.

**Features:**
* Sync transaction data from Plaid to your local machine
* Integration with [Plaid Link][Plaid Link]
* Customize beancount transactions with a lisp-like scripting language

## Example
[![asciicast](https://asciinema.org/a/437024.png)](https://asciinema.org/a/437024)

Without any rules configured, expenses are not categorized and accounts ids are
used in records.

You can optionally customize the output records in a variety of ways. See
[Ketos](https://github.com/murarth/ketos) for a breakdown of the scripting language.

__Default, no scripting rules applied.__
```
2021-09-16 * "AUTOMATIC PAYMENT - THANK"
    Expenses:Unknown  2078.50 USD
    NEBoJDJxmxhdoaBdAB7eueRQveKzqyFWao3nD

2021-09-16 * "Madison Bicycle Shop"
    Expenses:Unknown  500.00 USD
    NEBoJDJxmxhdoaBdAB7eueRQveKzqyFWao3nD

2021-09-16 * "KFC"
    Expenses:Unknown  500.00 USD
    NEBoJDJxmxhdoaBdAB7eueRQveKzqyFWao3nD

2021-09-17 * "Tectra Inc"
    Expenses:Unknown  500.00 USD
    NEBoJDJxmxhdoaBdAB7eueRQveKzqyFWao3nD

2021-09-20 * "Uber 072515 SF**POOL**"
    Expenses:Unknown  6.33 USD
    dLkx161njniBvw6B36LGIWJEvWnmjgfZ9lkD1
```

__Output with [scripting rules](transform.keto)__.
```
2021/09/16 * "AUTOMATIC PAYMENT - THANK"
    Expenses:Unknown  2078.50 USD
    Liabilities:Plaid-Credit-Card

2021/09/16 * "Madison Bicycle Shop"
    Expenses:Shopping  500.00 USD
    Liabilities:Plaid-Credit-Card

2021/09/16 * "KFC"
    Expenses:Food:Restaurant  500.00 USD
    Liabilities:Plaid-Credit-Card

2021/09/17 * "Tectra Inc"
    Expenses:Shopping  500.00 USD
    Liabilities:Plaid-Credit-Card

2021/09/20 * "Uber 072515 SF**POOL**"
    Expenses:Transportation:Rideshare  6.33 USD
    Assets:Plaid-Checking
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
clerk requires a [configuration file](clerk.toml) and optionally one
or more [keto scripts](transform.keto) for processing transactions into beancount
entries. The script in this repository highlights some features of scripting
provides such as __categorizing, aliasing, and regex-based matching__.

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
# Initializes a configuration file with interactive shell prompts.
clerk init

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
# Prints out beancount records for each transaction, can be filtered out by date using
# the --begin and --until flags respectively.
clerk txn print

# Pulls all transaction data for all accounts tracked in the given time range. By
# default, this command pulls the last 2 weeks of transactions. Time range can be
# manipulated using the --begin and --until commands.
clerk txn sync

# Filtering the first two weeks of September.
clerk txn sync --begin 2021-09-01 --until 2021-09-14
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
- [ ] Expand upstream data sources (e.g. csv imports, Stripe, etc)
- [ ] Expand on storage options (e.g. encryption / remote-storage)
- [ ] Open up scripting options by supporting WASM extensions

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
