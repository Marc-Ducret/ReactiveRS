use std::rc::Rc;
use std::cell::RefCell;


//   ____            _   _                   _   _
//  / ___|___  _ __ | |_(_)_ __  _   _  __ _| |_(_) ___  _ __
// | |   / _ \| '_ \| __| | '_ \| | | |/ _` | __| |/ _ \| '_ \
// | |__| (_) | | | | |_| | | | | |_| | (_| | |_| | (_) | | | |
//  \____\___/|_| |_|\__|_|_| |_|\__,_|\__,_|\__|_|\___/|_| |_|


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

impl<C, V> Continuation<V> for Pause<C>
    where C: Continuation<V> + 'static, V: 'static {
    fn call(self, runtime: &mut Runtime, value: V) {
        let c = self.continuation;
        runtime.on_next_instant(Box::new(|run: &mut Runtime, _| c.call(run, value)));
    }

    fn call_box(self: Box<Self>, runtime: &mut Runtime, value: V) {
        (*self).call(runtime, value);
    }
}


//  ____
// |  _ \ _ __ ___   ___ ___  ___ ___
// | |_) | '__/ _ \ / __/ _ \/ __/ __|
// |  __/| | | (_) | (_|  __/\__ \__ \
// |_|   |_|  \___/ \___\___||___/___/


/// A reactive process.
pub trait Process: 'static {
    /// The value created by the process.
    type Value;

    /// Executes the reactive process in the runtime, calls `next` with the resulting value.
    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value>;

    fn map<F, V2>(self, map: F) -> Map<Self, F> where Self: Sized, F: FnOnce(V2) -> Self::Value + 'static {
        Map { continuation: self, map }
    }

    fn pause(self) -> Pause<Self> where Self: Sized + 'static {
        Pause { continuation: self }
    }

    fn flatten(self) -> Flatten<Self> where Self: Sized, Self::Value: Process {
        Flatten { process: self }
    }
}

pub struct Value<T> {
    val : T
}

impl<T : 'static> Process for Value<T> {
    type Value = T;
    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        next.call(runtime, self.val)
    }
}

pub fn value<T>(val: T) -> Value<T> {
    Value {val}
}

pub struct Flatten<P> {
    process : P
}

impl<P> Process for Flatten<P>
    where P : Process + 'static, P::Value : Process {

    type Value = <P::Value as Process>::Value;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        self.process.call(runtime, |runtime: &mut Runtime, p: P::Value| p.call(runtime, next));
    }
}

impl<F, V2, P> Process for Map<P, F>
    where P : Process, F: FnOnce(P::Value) -> V2 + 'static
{
    type Value = V2;
    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        //self.continuation is a process
        let f = self.map;
        (self.continuation).call(runtime, move |runtime : &mut Runtime, x| (next.call(runtime, f(x))))
    }
}

impl<P> Process for Pause<P> where P : Process {
    type Value = P::Value;
    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        //self.continuation is a process
        let process = self.continuation;
        runtime.on_next_instant(Box::new(|run: &mut Runtime, x| process.call(run, next)))
    }
}


//  ____              _   _
// |  _ \ _   _ _ __ | |_(_)_ __ ___   ___
// | |_) | | | | '_ \| __| | '_ ` _ \ / _ \
// |  _ <| |_| | | | | |_| | | | | | |  __/
// |_| \_\\__,_|_| |_|\__|_|_| |_| |_|\___|


/// Runtime for executing reactive continuations.
pub struct Runtime {
    current_instant: Vec<Box<Continuation<()>>>,
    end_instant: Vec<Box<Continuation<()>>>,
    next_current_instant: Vec<Box<Continuation<()>>>,
    next_end_instant: Vec<Box<Continuation<()>>>,
}

impl Runtime {
    /// Creates a new `Runtime`.
    pub fn new() -> Self {
        Runtime {
            current_instant: Vec::new(),
            end_instant: Vec::new(),
            next_current_instant: Vec::new(),
            next_end_instant: Vec::new(),
        }
    }

    /// Executes instants until all work is completed.
    pub fn execute(&mut self) {
        while self.instant() {}
    }

    /// Executes a single instant to completion. Indicates if more work remains to be done.
    pub fn instant(&mut self) -> bool {
        while let Some(cont) = self.current_instant.pop() {
            cont.call_box(self, ());
        }
        std::mem::swap(&mut self.current_instant, &mut self.next_current_instant);
        std::mem::swap(&mut self.end_instant, &mut self.next_end_instant);
        while let Some(cont) = self.next_end_instant.pop() {
            cont.call_box(self, ());
        }

        (!self.current_instant.is_empty())
     || (!self.end_instant.is_empty())
     || (!self.next_end_instant.is_empty())
    }

    /// Registers a continuation to execute on the current instant.
    fn on_current_instant(&mut self, c: Box<Continuation<()>>) {
        self.current_instant.push(c);
    }

    /// Registers a continuation to execute at the next instant.
    fn on_next_instant(&mut self, c: Box<Continuation<()>>) {
        self.next_current_instant.push(c);
    }

    /// Registers a continuation to execute at the end of the instant. Runtime calls for `c`
    /// behave as if they where executed during the next instant.
    fn on_end_of_instant(&mut self, c: Box<Continuation<()>>) {
        self.end_instant.push(c);
    }
}


//  _____         _
// |_   _|__  ___| |_ ___
//   | |/ _ \/ __| __/ __|
//   | |  __/\__ \ |_\__ \
//   |_|\___||___/\__|___/


#[test]
fn question2() {
    let n = Rc::new(RefCell::new(0));
    let nn = n.clone();
    let mut runtime = Runtime::new();
    let cont_print = Box::new(move |_run :&mut Runtime, ()| *nn.borrow_mut() = 42);
    let cont_wait = Box::new(|run :&mut Runtime, ()| run.on_next_instant(cont_print));
    runtime.on_current_instant(cont_wait);
    assert_eq!(*n.borrow(), 0);
    assert!(runtime.instant());
    assert_eq!(*n.borrow(), 0);
    assert!(!runtime.instant());
    assert_eq!(*n.borrow(), 42);
}

#[test]
fn question5() {
    let n = Rc::new(RefCell::new(0));
    let nn = n.clone();
    let mut runtime = Runtime::new();
    let cont_print = Box::new(move |_run :&mut Runtime, ()| *nn.borrow_mut() = 42);
    let cont_wait = Box::new(cont_print.pause());
    runtime.on_current_instant(cont_wait);
    assert_eq!(*n.borrow(), 0);
    assert!(runtime.instant());
    assert_eq!(*n.borrow(), 0);
    assert!(!runtime.instant());
    assert_eq!(*n.borrow(), 42);
}

#[test]
fn test_process1() {
    let n = Rc::new(RefCell::new(0));
    let nn = n.clone();
    let mut runtime = Runtime::new();
    let p = value(value(42));

    assert_eq!(*n.borrow(), 0);
    p.flatten().call(&mut runtime, move |_: &mut Runtime, val| *nn.borrow_mut() = val);
    assert_eq!(*n.borrow(), 42);
}

fn main() {
}
