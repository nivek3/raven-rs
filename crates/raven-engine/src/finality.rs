//! Shared processing-height policy for append, gap recovery and reorg replay.

/// Processing delay, not protocol finality. Both policies require canonical checks,
/// rollback and replay. Confirmations counts successors: head 100 with 12 permits 88.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum FinalityPolicy {
    #[default]
    Head,
    Confirmations(u64),
}

impl FinalityPolicy {
    /// None means even genesis lacks enough successors; never saturate to zero.
    pub fn processable_height(self, head: u64) -> Option<u64> {
        match self {
            Self::Head => Some(head),
            Self::Confirmations(confirmations) => head.checked_sub(confirmations),
        }
    }

    /// Returns whether this block height is eligible under the observed head.
    pub(crate) fn permits(self, number: u64, head: u64) -> bool {
        self.processable_height(head)
            .is_some_and(|height| number <= height)
    }
}
