use async_trait::async_trait;

#[async_trait]
pub trait RuntimeQueue {
    type Item: Clone + Send + Sync + 'static;

    async fn list(&self) -> Vec<Self::Item>;
}
