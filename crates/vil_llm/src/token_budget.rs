//! Token budget tracking and enforcement.

#[derive(Debug)]
pub struct TokenBudget {
    limit: u64,
    used: u64,
}

impl TokenBudget {
    pub fn new(limit: u64) -> Self {
        Self { limit, used: 0 }
    }

    pub fn unlimited() -> Self {
        Self { limit: 0, used: 0 }
    }

    pub fn add_usage(&mut self, tokens: u64) {
        self.used += tokens;
    }

    pub fn is_exceeded(&self) -> bool {
        self.limit > 0 && self.used >= self.limit
    }

    pub fn remaining(&self) -> u64 {
        if self.limit == 0 {
            u64::MAX
        } else {
            self.limit.saturating_sub(self.used)
        }
    }

    pub fn used(&self) -> u64 {
        self.used
    }
    pub fn limit(&self) -> u64 {
        self.limit
    }

    pub fn reset(&mut self) {
        self.used = 0;
    }
}
