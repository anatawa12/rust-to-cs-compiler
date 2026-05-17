use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;

pub struct IdMap<T> {
    prefix: &'static str,
    inner: RefCell<IdMapInner<T>>,
}
struct IdMapInner<T> {
    next_id: usize,
    id_map: HashMap<T, usize>,
}

impl<T: Hash + Eq + Clone> IdMap<T> {
    pub fn new(prefix: &'static str) -> Self {
        Self {
            prefix,
            inner: RefCell::new(IdMapInner {
                next_id: 0,
                id_map: HashMap::new(),
            }),
        }
    }

    pub fn id_name(&self, item: &T) -> String {
        format!("{}_{}", self.prefix, self.get_id(item))
    }

    pub fn get_id(&self, item: &T) -> usize {
        let mut this = self.inner.borrow_mut();
        let this = &mut *this;
        *this.id_map.entry(item.clone()).or_insert_with(|| {
            let id = this.next_id;
            this.next_id += 1;
            id
        })
    }
}
