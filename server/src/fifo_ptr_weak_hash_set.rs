use std::{collections::VecDeque, hash::RandomState, rc::{Rc, Weak}};
use weak_table::{PtrWeakHashSet};

#[derive(Debug)]
pub struct FifoPtrWeakHashSet<T> {
    set: PtrWeakHashSet<Weak<T>, RandomState>,
    queue: VecDeque<Weak<T>>,
}

impl<T> FifoPtrWeakHashSet<T>
{
    pub fn new() -> Self {
        Self {
            set: PtrWeakHashSet::new(),
            queue: VecDeque::new(),
        }
    }

    pub fn insert(&mut self, v: Rc<T>) {
        if !self.set.insert(v.clone()) { //it returns true if absent (wrong doc)
            self.queue.push_back(Rc::downgrade(&v));
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = Rc<T>> {
        self.queue.iter().filter_map(|weak| weak.upgrade())
    }

    pub fn contains(&self, v: &Rc<T>) -> bool {
        self.set.contains(v)
    }

    pub fn clear(&mut self) {
        self.set.clear();
        self.queue.clear();
    }

    pub fn remove(&mut self, v: &Rc<T>) -> bool {
        if self.set.remove(v) {
            let weak = Rc::downgrade(v);
            let pos = self.queue.iter().position(|x| Weak::ptr_eq(x, &weak));
            if let Some(pos) = pos {
                self.queue.remove(pos);
            }
            return true
        }
        false
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }
}
