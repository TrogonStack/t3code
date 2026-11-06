/// Back/forward over visited projects, the GPUI stand-in for the web's route history.
#[derive(Default)]
pub struct History {
    visited: Vec<String>,
    index: usize,
}

impl History {
    /// Like a route push: drops anything ahead and ignores revisiting the current entry.
    pub fn visit(&mut self, id: String) {
        if self.current() == Some(id.as_str()) {
            return;
        }
        if !self.visited.is_empty() {
            self.visited.truncate(self.index + 1);
            self.index += 1;
        }
        self.visited.push(id);
    }

    pub fn can_back(&self) -> bool {
        self.index > 0
    }

    pub fn can_forward(&self) -> bool {
        self.index + 1 < self.visited.len()
    }

    pub fn back(&mut self) -> Option<&str> {
        self.can_back().then(|| self.index -= 1)?;
        self.current()
    }

    pub fn forward(&mut self) -> Option<&str> {
        self.can_forward().then(|| self.index += 1)?;
        self.current()
    }

    fn current(&self) -> Option<&str> {
        self.visited.get(self.index).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_back_and_forward_and_a_new_visit_drops_the_forward_entries() {
        let mut history = History::default();
        assert!(!history.can_back() && !history.can_forward());
        history.visit("a".into());
        history.visit("a".into());
        history.visit("b".into());
        history.visit("c".into());
        assert_eq!(history.back(), Some("b"));
        assert_eq!(history.back(), Some("a"));
        assert_eq!(history.back(), None);
        assert_eq!(history.forward(), Some("b"));
        history.visit("d".into());
        assert!(!history.can_forward());
        assert_eq!(history.back(), Some("b"));
        assert_eq!(history.back(), Some("a"));
    }
}
