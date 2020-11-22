#![feature(once_cell)]

use std::cell::Cell;
use std::lazy::{OnceCell, SyncOnceCell};
use std::sync::mpsc;
use std::sync::{Arc, Weak};

pub struct LazyField<'a, S, T>(Arc<LazyFieldInner<'a, S, T>>);

struct LazyFieldInner<'a, S, T> {
    value: SyncOnceCell<T>,
    constructor: Cell<Box<dyn FnOnce(&S) -> T + 'a>>,
    holder: Weak<OnceCell<S>>,
}

impl<S, T> LazyFieldInner<'_, S, T> {
    fn get(&self) -> &T {
        self.value.get_or_init(|| {
            let f = self
                .constructor
                .replace(Box::new(|_| panic!("Field already constructed")));
            let obj = &*self
                .holder
                .upgrade()
                .unwrap_or_else(|| panic!("Object deleted"));
            match obj.get() {
                Some(ref x) => f(x),
                None => panic!("Object not constructed"),
            }
        })
    }
}

impl<S, T> LazyField<'_, S, T> {
    pub fn get(&self) -> &T {
        self.0.get()
    }
    pub fn into_inner(self) -> T {
        Arc::try_unwrap(self.0)
            .unwrap_or_else(|_| panic!("Other references to field being held?"))
            .value
            .into_inner()
            .unwrap_or_else(|| panic!("Field uninitialized"))
    }
}

trait IsField<S> {
    fn resolve(&self);
}

impl<'a, S, T> IsField<S> for LazyFieldInner<'a, S, T> {
    fn resolve(&self) {
        self.get();
    }
}

pub struct Register<'a, S> {
    holder: Weak<OnceCell<S>>,
    fields: mpsc::Sender<Weak<dyn IsField<S> + 'a>>,
}

impl<'a, S: 'a> Register<'a, S> {
    pub fn field<T: 'a, F: FnOnce(&S) -> T + 'a>(&self, f: F) -> LazyField<'a, S, T> {
        let result = Arc::new(LazyFieldInner::<'a, S, T> {
            value: SyncOnceCell::new(),
            constructor: Cell::new(Box::new(f)),
            holder: self.holder.clone(),
        });
        self.fields
            .send(Arc::downgrade(&result) as Weak<dyn IsField<S>>)
            .unwrap();
        LazyField(result)
    }
}

pub fn with_lazy_fields<'a, S, F: FnOnce(&mut Register<'a, S>) -> S>(f: F) -> S {
    let holder = Arc::new(OnceCell::new());
    let (sender, receiver) = mpsc::channel();
    let mut reg = Register {
        holder: Arc::downgrade(&holder),
        fields: sender,
    };
    let res = f(&mut reg);
    holder
        .set(res)
        .unwrap_or_else(|_| panic!("Already initialized?"));
    for field in receiver {
        if let Some(x) = field.upgrade() {
            x.resolve()
        }
    }
    Arc::try_unwrap(holder)
        .unwrap_or_else(|_| panic!("Other references to holder being held?"))
        .take()
        .unwrap_or_else(|| panic!("Uninitialized after calling set?"))
}
