#[derive(Debug, Default)]
pub struct TeamContext {
    pub reviewers: Vec<String>,
    pub pending_reviews: usize,
    pub discussion_handles: Vec<String>,
}
