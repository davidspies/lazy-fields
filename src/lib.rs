#![feature(once_cell)]

use std::cell::Cell;
use std::lazy::SyncOnceCell;
use std::sync::mpsc;
use std::sync::{Arc, Weak};

pub struct LazyField<'a, S, T>(Arc<LazyFieldInner<'a, S, T>>);

struct LazyFieldInner<'a, S, T> {
    value: SyncOnceCell<T>,
    constructor: Cell<Box<dyn FnOnce(&S) -> T + 'a>>,
    holder: Cell<Weak<S>>,
}

impl<S, T> LazyFieldInner<'_, S, T> {
    fn get(&self) -> &T {
        self.value.get_or_init(|| {
            let f = self.constructor.replace(Box::new(|_| {
                panic!("Already constructed! (Constructor panicked?)")
            }));
            f(&*self
                .holder
                .replace(Weak::new())
                .upgrade()
                .unwrap_or_else(|| panic!("Object not constructed")))
        })
    }
}

impl<S, T> LazyField<'_, S, T> {
    pub fn get(&self) -> &T {
        self.0.get()
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
    fn resolve(&self);
    fn set_holder(&self, holder: Weak<S>);
}

impl<'a, S, T> IsField<S> for LazyFieldInner<'a, S, T> {
    fn resolve(&self) {
        self.get();
    }
    fn set_holder(&self, holder: Weak<S>) {
        self.holder.replace(holder);
    }
}

pub struct Register<'a, S>(mpsc::Sender<Weak<dyn IsField<S> + 'a>>);

impl<'a, S: 'a> Register<'a, S> {
    pub fn field<T: 'a, F: FnOnce(&S) -> T + 'a>(&self, f: F) -> LazyField<'a, S, T> {
        let result = Arc::new(LazyFieldInner::<'a, S, T> {
            value: SyncOnceCell::new(),
            constructor: Cell::new(Box::new(f)),
            holder: Cell::new(Weak::new()),
        });
        self.0
            .send(Arc::downgrade(&result) as Weak<dyn IsField<S>>)
            .unwrap();
        LazyField(result)
    }
}

pub fn with_lazy_fields<'a, S, F: FnOnce(&mut Register<'a, S>) -> S>(f: F) -> S {
    let (sender, receiver) = mpsc::channel();
    let mut reg = Register(sender);
    let res = Arc::new(f(&mut reg));
    let received = receiver.into_iter().collect::<Vec<_>>();
    for field in received.iter() {
        if let Some(x) = field.upgrade() {
            x.set_holder(Arc::downgrade(&res));
        }
    }
    for field in received {
        if let Some(x) = field.upgrade() {
            x.resolve()
        }
    }
    Arc::try_unwrap(res).unwrap_or_else(|_| panic!("Unreachable! Other arcs still held"))
}
