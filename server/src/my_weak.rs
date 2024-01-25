use std::sync::{Arc, Weak};
use std::hash::{Hash, Hasher};

//my_weak is a structure that hold a value as Weak, but can be used in a HashSet
#[derive(Debug)]
pub struct MyWeak<T> {
    weak: Weak<T>,
}

impl <T> MyWeak<T> {
    pub fn new(weak: Weak<T>) -> Self {
        Self {
            weak: weak,
        }
    }

    pub fn upgrade(&self) -> Option<Arc<T>> {
        self.weak.upgrade()
    }
}


impl<T> Eq for MyWeak<T> {}

impl<T> PartialEq for MyWeak<T> {
    fn eq(&self, other: &Self) -> bool {
        self.weak.ptr_eq(&other.weak)
    }
}

impl<T> Hash for MyWeak<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.weak.as_ptr().hash(state);
    }
}