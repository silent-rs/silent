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
