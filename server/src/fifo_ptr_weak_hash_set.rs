use std::{collections::{HashSet, VecDeque}, hash::Hash};

#[derive(Debug)]
pub struct FifoWeakHashSet<T: Copy + Eq + Hash> {
    set: HashSet<T>,
    queue: VecDeque<T>,
}

// @arena-next
impl<T: Copy + Eq + Hash> FifoWeakHashSet<T> {
    pub fn new() -> Self {
        Self {
            set: HashSet::new(),
            queue: VecDeque::new(),
        }
    }

    // @arena: previous implementation, based on PtrWeakHashSet, removed expired keys (from the set only) on insertion.
    // This one does not.
    pub fn insert(&mut self, v: T) {
        // @arena: unlike PtrWeakHashSet, HashSet's doc is correct.
        // insert returns true if the value was not present (PtrWeakHashSet's behavior is the opposite)
        if self.set.insert(v) {
            self.queue.push_back(v);
        }
    }

    // @arena-todo: stale weaks linger forever in the queue (like previous implementation)
    pub fn iter_valid(&self, is_valid: impl Fn(&T) -> bool) -> impl Iterator<Item = T> {
        self.queue.iter().filter(move |&weak| is_valid(weak)).copied()
    }

    pub fn contains(&self, v: &T) -> bool {
        self.set.contains(v)
    }

    pub fn clear(&mut self) {
        self.set.clear();
        self.queue.clear();
    }

    pub fn remove(&mut self, v: &T) -> bool {
        if self.set.remove(v) {
            if let Some(pos) = self.queue.iter().position(|x| x == v) {
                self.queue.remove(pos);
            }
            return true
        }
        false
    }

    // @arena: like previous implementation, invalid weaks are not removed from
    // the set, so is_empty and len return values may be wrong
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }
}
