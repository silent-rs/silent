use async_trait::async_trait;

/// 定时任务存储Trait
#[async_trait]
trait Storage {
    async fn load(&mut self);
    async fn save(&mut self);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dummy;

    #[async_trait]
    impl Storage for Dummy {
        async fn load(&mut self) {}
        async fn save(&mut self) {}
    }

    #[tokio::test]
    async fn test_storage_trait_compiles() {
        let mut d = Dummy;
        d.load().await;
        d.save().await;
    }
}
