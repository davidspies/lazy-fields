#![feature(once_cell)]

use std::cell::Cell;
use std::lazy::SyncOnceCell;
use std::sync::{Arc, Weak};

pub struct LazyField<'a, S, T>(Arc<LazyFieldInner<'a, S, T>>);

struct LazyFieldInner<'a, S, T> {
    value: SyncOnceCell<T>,
    constructor: Cell<Box<dyn FnOnce(&S) -> T + 'a>>,
}

impl<S, T> LazyFieldInner<'_, S, T> {
    fn get(&self, holder: &S) -> &T {
        self.value.get_or_init(|| {
            let f = self
                .constructor
                .replace(Box::new(|_| panic!("Already constructed! (Constructor panicked?)")));
            f(holder)
        })
    }
}

impl<S, T> LazyField<'_, S, T> {
    pub fn get(&self, holder: &S) -> &T {
        self.0.get(holder)
    }
    pub fn into_inner(self) -> T {
        Arc::try_unwrap(self.0)
            .unwrap_or_else(|_| panic!("Other references being held?"))
            .value
            .into_inner()
            .unwrap_or_else(|| panic!("Uninitialized!"))
    }
}

trait IsField<S> {
    fn resolve(&self, holder: &S);
}

impl<'a, S, T> IsField<S> for LazyFieldInner<'a, S, T> {
    fn resolve(&self, holder: &S) {
        self.get(holder);
    }
}

pub struct Register<'a, S>(Vec<Weak<dyn IsField<S> + 'a>>);

impl<'a, S: 'a> Register<'a, S> {
    pub fn field<T: 'a, F: FnOnce(&S) -> T + 'a>(&mut self, f: F) -> LazyField<'a, S, T> {
        let result = Arc::new(LazyFieldInner::<'a, S, T> {
            value: SyncOnceCell::new(),
            constructor: Cell::new(Box::new(f)),
        });
        self.0.push(Arc::downgrade(&result) as Weak<dyn IsField<S>>);
        LazyField(result)
    }
}

pub fn lazy<'a, S, F: FnOnce(&mut Register<'a, S>) -> S>(f: F) -> S {
    let mut reg = Register(Vec::new());
    let res = f(&mut reg);
    for field in reg.0 {
        if let Some(x) = field.upgrade() {
            x.resolve(&res)
        }
    }
    res
}
