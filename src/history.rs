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
    pub fn latest(&self) -> Option<T> {
        self.buf.back().copied()
    }
    pub fn iter(&self) -> impl Iterator<Item = T> + '_ {
        self.buf.iter().copied()
    }
    pub fn len(&self) -> usize {
        self.buf.len()
    }
    pub fn cap(&self) -> usize {
        self.cap
    }
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
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
        assert_eq!(h.iter().collect::<Vec<_>>(), vec![3, 4, 5]);
        assert_eq!(h.latest(), Some(5));
        assert_eq!(h.len(), 3);
    }

    #[test]
    fn empty() {
        let h: History<f32> = History::new(4);
        assert_eq!(h.latest(), None);
        assert_eq!(h.iter().count(), 0);
    }
}
