use std::io::Write;

use anyhow::Result;
use tabwriter::TabWriter;

use crate::core::Account;

pub fn print_accounts<T: std::io::Write>(wr: T, ins_name: &str, accounts: &[Account]) -> Result<()> {
    let mut tw = TabWriter::new(wr);
    writeln!(tw, "Institution\tAccount\tAccount ID\tType\tStatus")?;

    for account in accounts.iter() {
        writeln!(
            tw,
            "{}\t{}\t{}\t{:?}",
            ins_name,
            account.name,
            account.id,
            account.ty,
        )?;
    }

    tw.flush()?;

    Ok(())
}
