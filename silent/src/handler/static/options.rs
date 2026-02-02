#[derive(Clone, Debug, Default)]
pub struct StaticOptions {
    pub enable_compression: bool,
    pub directory_listing: bool,
}

impl StaticOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enable_compression(mut self, enable: bool) -> Self {
        self.enable_compression = enable;
        self
    }

    pub fn with_compression(mut self) -> Self {
        self.enable_compression = true;
        self
    }

    pub fn enable_directory_listing(mut self, enable: bool) -> Self {
        self.directory_listing = enable;
        self
    }

    pub fn with_directory_listing(mut self) -> Self {
        self.directory_listing = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== 构造函数测试 ====================

    #[test]
    fn test_static_options_new() {
        let options = StaticOptions::new();
        assert!(!options.enable_compression);
        assert!(!options.directory_listing);
    }

    #[test]
    fn test_static_options_default() {
        let options = StaticOptions::default();
        assert!(!options.enable_compression);
        assert!(!options.directory_listing);
    }

    // ==================== Compression 相关测试 ====================

    #[test]
    fn test_enable_compression_true() {
        let options = StaticOptions::new().enable_compression(true);
        assert!(options.enable_compression);
        assert!(!options.directory_listing);
    }

    #[test]
    fn test_enable_compression_false() {
        let options = StaticOptions::new()
            .enable_compression(true)
            .enable_compression(false);
        assert!(!options.enable_compression);
    }

    #[test]
    fn test_with_compression() {
        let options = StaticOptions::new().with_compression();
        assert!(options.enable_compression);
        assert!(!options.directory_listing);
    }

    #[test]
    fn test_enable_compression_chain() {
        let options = StaticOptions::new()
            .enable_compression(true)
            .enable_compression(false)
            .enable_compression(true);
        assert!(options.enable_compression);
    }

    // ==================== Directory Listing 相关测试 ====================

    #[test]
    fn test_enable_directory_listing_true() {
        let options = StaticOptions::new().enable_directory_listing(true);
        assert!(!options.enable_compression);
        assert!(options.directory_listing);
    }

    #[test]
    fn test_enable_directory_listing_false() {
        let options = StaticOptions::new()
            .enable_directory_listing(true)
            .enable_directory_listing(false);
        assert!(!options.directory_listing);
    }

    #[test]
    fn test_with_directory_listing() {
        let options = StaticOptions::new().with_directory_listing();
        assert!(!options.enable_compression);
        assert!(options.directory_listing);
    }

    #[test]
    fn test_enable_directory_listing_chain() {
        let options = StaticOptions::new()
            .enable_directory_listing(true)
            .enable_directory_listing(false)
            .enable_directory_listing(true);
        assert!(options.directory_listing);
    }

    // ==================== 组合功能测试 ====================

    #[test]
    fn test_combined_options() {
        let options = StaticOptions::new()
            .enable_compression(true)
            .enable_directory_listing(true);
        assert!(options.enable_compression);
        assert!(options.directory_listing);
    }

    #[test]
    fn test_builder_pattern() {
        let options = StaticOptions::new()
            .with_compression()
            .with_directory_listing();
        assert!(options.enable_compression);
        assert!(options.directory_listing);
    }

    #[test]
    fn test_override_options() {
        let options = StaticOptions::new()
            .with_compression()
            .with_directory_listing()
            .enable_compression(false)
            .enable_directory_listing(false);
        assert!(!options.enable_compression);
        assert!(!options.directory_listing);
    }

    // ==================== Trait 实现测试 ====================

    #[test]
    fn test_clone_trait() {
        let options1 = StaticOptions::new()
            .with_compression()
            .with_directory_listing();
        let options2 = options1.clone();
        assert_eq!(options1.enable_compression, options2.enable_compression);
        assert_eq!(options1.directory_listing, options2.directory_listing);
    }

    #[test]
    fn test_debug_trait() {
        let options = StaticOptions::new()
            .with_compression()
            .with_directory_listing();
        let debug_str = format!("{:?}", options);
        assert!(debug_str.contains("StaticOptions"));
        assert!(debug_str.contains("enable_compression"));
        assert!(debug_str.contains("directory_listing"));
    }
}
