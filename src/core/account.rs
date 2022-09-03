use rplaid::model::{self, AccountType};

#[derive(Debug, Clone)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub ty: String,
}

impl From<model::Account> for Account {
    fn from(model: model::Account) -> Self {
        let ty = match model.r#type {
            AccountType::Credit | AccountType::Loan => "CREDIT_NORMAL",
            AccountType::Depository | AccountType::Investment | AccountType::Brokerage => "DEBIT_NORMAL",
            _ => unimplemented!(),
        };

        Self {
            id: model.account_id,
            name: model.name,
            ty: ty.into(),
        }
    }
}
