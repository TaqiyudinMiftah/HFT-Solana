#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BankIdentity {
    pub generation: u64,
    pub slot: u64,
    pub bank_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountUpdate {
    pub pubkey: [u8; 32],
    pub owner: [u8; 32],
    pub slot: u64,
    pub write_version: u64,
    pub generation: u64,
    pub bank_id: Option<u64>,
    pub is_startup: bool,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FeedEvent {
    Account(AccountUpdate),
    /// A reconnect can invalidate updates from partially delivered banks.
    ///
    /// Consumers that retain bank-scoped state must remove these exact bank
    /// identities before applying replacement updates from the stream.
    DiscardBanks {
        banks: Vec<BankIdentity>,
        reason: String,
    },
}
