use std::rc::Rc;
use std::cell::RefCell;

/// A reactive continuation awaiting a value of type `V`. For the sake of simplicity,
/// continuation must be valid on the static lifetime.
pub trait Continuation<V>: 'static {
    /// Calls the continuation.
    fn call(self, runtime: &mut Runtime, value: V);

    /// Calls the continuation. Works even if the continuation is boxed.
    ///
    /// This is necessary because the size of a value must be known to unbox it. It is
    /// thus impossible to take the ownership of a `Box<Continuation>` whitout knowing the
    /// underlying type of the `Continuation`.
    fn call_box(self: Box<Self>, runtime: &mut Runtime, value: V);

    /// Creates a new continuation that applies a function to the input value before
    /// calling `Self`.
    fn map<F, V2>(self, map: F) -> Map<Self, F> where Self: Sized, F: FnOnce(V2) -> V + 'static {
        Map { continuation: self, map }
    }

    fn pause(self) -> Pause<Self> where Self: Sized + 'static {
        Pause { continuation: self }
    }
}

impl<V, F> Continuation<V> for F where F: FnOnce(&mut Runtime, V) + 'static {
    fn call(self, runtime: &mut Runtime, value: V)  {
        self(runtime, value);
    }

    fn call_box(self: Box<Self>, runtime: &mut Runtime, value: V) {
        (*self).call(runtime, value);
    }
}

/// A continuation that applies a function before calling another continuation.
pub struct Map<C, F> { continuation: C, map: F }

impl<C, F, V1, V2> Continuation<V1> for Map<C, F>
    where C: Continuation<V2>, F: FnOnce(V1) -> V2 + 'static
{
    fn call(self, runtime: &mut Runtime, value: V1) {
        self.continuation.call(runtime, (self.map)(value));
    }

    fn call_box(self: Box<Self>, runtime: &mut Runtime, value: V1) {
        (*self).call(runtime, value);
    }
}

pub struct Pause<C> { continuation: C }

impl<C> Continuation<()> for Pause<C>
    where C: Continuation<()> + 'static {
    fn call(self, runtime: &mut Runtime, value: ()) {
        runtime.on_next_instant(Box::new(self.continuation));
    }

    fn call_box(self: Box<Self>, runtime: &mut Runtime, value: ()) {
        (*self).call(runtime, value);
    }
}

/// Runtime for executing reactive continuations.
pub struct Runtime {
    currentInstant : Vec<Box<Continuation<()>>>,
    endInstant : Vec<Box<Continuation<()>>>,
    nextCurrentInstant : Vec<Box<Continuation<()>>>,
    nextEndInstant : Vec<Box<Continuation<()>>>,
}

impl Runtime {
    /// Creates a new `Runtime`.
    pub fn new() -> Self {
        Runtime {
            currentInstant : Vec::new(),
            endInstant : Vec::new(),
            nextCurrentInstant : Vec::new(),
            nextEndInstant : Vec::new(),
        }
    }

    /// Executes instants until all work is completed.
    pub fn execute(&mut self) {
        while self.instant() {}
    }

    /// Executes a single instant to completion. Indicates if more work remains to be done.
    pub fn instant(&mut self) -> bool {
        println!("instant");
        while let Some(cont) = self.currentInstant.pop() {
            cont.call_box(self, ());
        }
        std::mem::swap(&mut self.currentInstant, &mut self.nextCurrentInstant);
        std::mem::swap(&mut self.endInstant, &mut self.nextEndInstant);
        while let Some(cont) = self.nextEndInstant.pop() {
            cont.call_box(self, ());
        }

        (!self.currentInstant.is_empty())
     || (!self.endInstant.is_empty())
     || (!self.nextEndInstant.is_empty())
    }

    /// Registers a continuation to execute on the current instant.
    fn on_current_instant(&mut self, c: Box<Continuation<()>>) {
        self.currentInstant.push(c);
    }

    /// Registers a continuation to execute at the next instant.
    fn on_next_instant(&mut self, c: Box<Continuation<()>>) {
        self.nextCurrentInstant.push(c);
    }

    /// Registers a continuation to execute at the end of the instant. Runtime calls for `c`
    /// behave as if they where executed during the next instant.
    fn on_end_of_instant(&mut self, c: Box<Continuation<()>>) {
        self.endInstant.push(c);
    }
}

#[test]
fn question2() {
    let n = Rc::new(RefCell::new(0));
    let nn = n.clone();
    let mut runtime = Runtime::new();
    let contPrint = Box::new(move |run :&mut Runtime, ()| *nn.borrow_mut() = 42);
    let contWait = Box::new(|run :&mut Runtime, ()| run.on_next_instant(contPrint));
    runtime.on_current_instant(contWait);
    assert_eq!(*n.borrow(), 0);
    assert!(runtime.instant());
    assert_eq!(*n.borrow(), 0);
    assert!(!runtime.instant());
    assert_eq!(*n.borrow(), 42);
}

fn main() {
}
