/// Back/forward over visited routes, the GPUI stand-in for the web's route history.
pub struct History<T> {
    visited: Vec<T>,
    index: usize,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self {
            visited: Vec::new(),
            index: 0,
        }
    }
}

impl<T: PartialEq> History<T> {
    /// Like a route push: drops anything ahead and ignores revisiting the current entry.
    pub fn visit(&mut self, route: T) {
        if self.current() == Some(&route) {
            return;
        }
        if !self.visited.is_empty() {
            self.visited.truncate(self.index + 1);
            self.index += 1;
        }
        self.visited.push(route);
    }

    pub fn can_back(&self) -> bool {
        self.index > 0
    }

    pub fn can_forward(&self) -> bool {
        self.index + 1 < self.visited.len()
    }

    pub fn back(&mut self) -> Option<&T> {
        self.can_back().then(|| self.index -= 1)?;
        self.current()
    }

    pub fn forward(&mut self) -> Option<&T> {
        self.can_forward().then(|| self.index += 1)?;
        self.current()
    }

    fn current(&self) -> Option<&T> {
        self.visited.get(self.index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_back_and_forward_and_a_new_visit_drops_the_forward_entries() {
        let mut history = History::<&str>::default();
        assert!(!history.can_back() && !history.can_forward());
        history.visit("a");
        history.visit("a");
        history.visit("b");
        history.visit("c");
        assert_eq!(history.back(), Some(&"b"));
        assert_eq!(history.back(), Some(&"a"));
        assert_eq!(history.back(), None);
        assert_eq!(history.forward(), Some(&"b"));
        history.visit("d");
        assert!(!history.can_forward());
        assert_eq!(history.back(), Some(&"b"));
        assert_eq!(history.back(), Some(&"a"));
    }
}
