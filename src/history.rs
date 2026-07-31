use std::collections::VecDeque;

pub struct History<T: Copy> {
    buf: VecDeque<T>,
    cap: usize,
}

impl<T: Copy> History<T> {
    pub fn new(cap: usize) -> Self {
        Self { buf: VecDeque::with_capacity(cap), cap }
    }
    pub fn push(&mut self, v: T) {
        if self.buf.len() == self.cap {
            self.buf.pop_front();
        }
        self.buf.push_back(v);
    }
    /// Oldest to newest — the order every chart in `ui` draws in.
    pub fn iter(&self) -> impl Iterator<Item = T> + '_ {
        self.buf.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_cap_items_in_order() {
        let mut h = History::new(3);
        for v in [1, 2, 3, 4, 5] {
            h.push(v);
        }
        assert_eq!(h.iter().collect::<Vec<_>>(), vec![3, 4, 5], "oldest first, capped at cap");
    }

    #[test]
    fn empty() {
        let h: History<f32> = History::new(4);
        assert_eq!(h.iter().count(), 0);
    }
}
