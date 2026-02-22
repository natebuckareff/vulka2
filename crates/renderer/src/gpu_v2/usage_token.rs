pub struct UsageToken {
    used: bool,
}

impl UsageToken {
    pub fn new() -> Self {
        Self { used: false }
    }

    pub fn consume(&mut self) {
        self.used = true;
    }
}

impl Drop for UsageToken {
    fn drop(&mut self) {
        assert!(self.used, "usage token not used");
    }
}
