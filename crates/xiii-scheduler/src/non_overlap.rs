#[derive(Debug, Default)]
pub struct NonOverlapGuard {
    running: bool,
}

impl NonOverlapGuard {
    pub fn try_start(&mut self) -> bool {
        if self.running {
            false
        } else {
            self.running = true;
            true
        }
    }

    pub fn finish(&mut self) {
        self.running = false;
    }
}

#[cfg(test)]
mod tests {
    use super::NonOverlapGuard;

    #[test]
    fn guard_skips_overlap() {
        let mut guard = NonOverlapGuard::default();
        assert!(guard.try_start());
        assert!(!guard.try_start());
        guard.finish();
        assert!(guard.try_start());
    }
}
