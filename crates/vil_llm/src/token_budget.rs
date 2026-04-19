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
        self.used = self.used.saturating_add(tokens);
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn add_usage_saturates_on_overflow() {
        let mut budget = TokenBudget::new(u64::MAX);
        budget.add_usage(u64::MAX);
        budget.add_usage(1_000);
        assert_eq!(budget.used(), u64::MAX);
    }

    #[test]
    fn add_usage_accumulates_normally() {
        let mut budget = TokenBudget::new(1000);
        budget.add_usage(100);
        budget.add_usage(200);
        assert_eq!(budget.used(), 300);
    }
}
