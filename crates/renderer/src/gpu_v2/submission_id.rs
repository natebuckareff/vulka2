use anyhow::Result;

pub(crate) const SUBMISSION_ID_UNINITIALIZED: u64 = u64::MAX;
pub(crate) const SUBMISSION_ID_FAILED: u64 = u64::MAX - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SubmissionId(u64);

impl SubmissionId {
    pub(crate) fn new(id: u64) -> Result<Self> {
        assert!(id < SUBMISSION_ID_FAILED, "submission id overflow");
        Ok(SubmissionId(id))
    }

    pub fn is_set(&self) -> bool {
        !self.zero() && !self.uninitialized() && !self.failed()
    }

    pub fn zero(&self) -> bool {
        self.0 == 0
    }

    pub fn uninitialized(&self) -> bool {
        self.0 == SUBMISSION_ID_UNINITIALIZED
    }

    pub fn failed(&self) -> bool {
        self.0 == SUBMISSION_ID_FAILED
    }
}

impl Into<u64> for SubmissionId {
    fn into(self) -> u64 {
        self.0
    }
}
