use std::rc::Rc;
use std::cell::RefCell;
use std::cell::Cell;

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
    /// This is necessary because the size of a value must be known to un-box it. It is
    /// thus impossible to take the ownership of a `Box<Continuation>` without knowing the
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

    fn map<F, V2>(self, map: F) -> Map<Self, F> where Self: Sized, F: FnOnce(Self::Value) -> V2 + 'static {
        Map { continuation: self, map }
    }

    fn pause(self) -> Pause<Self> where Self: Sized + 'static {
        Pause { continuation: self }
    }

    fn flatten(self) -> Flatten<Self> where Self: Sized, Self::Value: Process {
        Flatten { process: self }
    }

    fn and_then<F, P>(self, then: F) -> Flatten<Map<Self, F>> where Self: Sized, F: FnOnce(Self::Value) -> P + 'static, P: Process {
        self.map(then).flatten()
    }
}

/// A process that can be executed multiple times, modifying its environment each time.
pub trait ProcessMut: Process {
    /// Executes the mutable process in the runtime, then calls `next` with the process and the
    /// process's return value.
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where
        Self: Sized, C: Continuation<(Self, Self::Value)>;

    fn while_loop<V>(self) -> While<Self> where Self: ProcessMut<Value = LoopStatus<V>>, Self: Sized {
        While {process: self}
    }
}

/// Indicates if a loop is finished.
pub enum LoopStatus<V> { Continue, Exit(V) }

pub fn execute_process<P>(p: P) -> P::Value where P: Process {
    let mut runtime = Runtime::new();
    let result = Rc::new(Cell::new(None));
    let result_ref = result.clone();
    runtime.on_current_instant(Box::new(|run: &mut Runtime, _|
    p.call(run, move |_: &mut Runtime, val| result_ref.set(Some(val)))));
    runtime.execute();
    if let Some(res) = result.replace(None) {
        return res;
    } else {
        panic!("No result from execute?!");
    }
}

pub struct Value<T> {
    val: T
}

impl<T: 'static> Process for Value<T> {
    type Value = T;
    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        next.call(runtime, self.val)
    }
}

impl<T: 'static> ProcessMut for Value<T> where T: Copy {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, Self::Value)> {
        let v = self.val.clone();
        next.call(runtime, (self, v))
    }
}

pub fn value<T>(val: T) -> Value<T> {
    Value {val}
}

pub struct Flatten<P> {
    process: P
}

impl<P> Process for Flatten<P>
    where P: Process + 'static, P::Value: Process {

    type Value = <P::Value as Process>::Value;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        self.process.call(runtime, |runtime: &mut Runtime, p: P::Value| p.call(runtime, next));
    }
}

impl<P> ProcessMut for Flatten<P>
    where P: ProcessMut + 'static, P::Value: ProcessMut {

    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, Self::Value)> {
        self.process.call_mut(runtime, |runtime: &mut Runtime, (process, p): (P, P::Value)|
            p.call_mut(runtime, next.map(|(p, v)| (process.flatten(), v)))
        );
    }
}

impl<F, V, P> Process for Map<P, F>
    where P: Process, F: FnOnce(P::Value) -> V + 'static  {
    type Value = V;
    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        //self.continuation is a process
        let f = self.map;
        (self.continuation).call(runtime, move |runtime: &mut Runtime, x| (next.call(runtime, f(x))))
    }
}

impl<F, V, P> ProcessMut for Map<P, F>
    where P: ProcessMut, F: FnMut(P::Value) -> V + 'static  {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, Self::Value)> {
        //self.continuation is a process
        let mut f: F = self.map;
        self.continuation.call_mut(runtime, move |runtime: &mut Runtime, (p, x): (P, P::Value)| {
            let y = f(x);
            next.call(runtime, (p.map(f), y))
        })
    }
}

impl<P> Process for Pause<P> where P: Process {
    type Value = P::Value;
    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        //self.continuation is a process
        let process = self.continuation;
        runtime.on_next_instant(Box::new(|run: &mut Runtime, _| process.call(run, next)))
    }
}

impl<P> ProcessMut for Pause<P> where P: ProcessMut {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, Self::Value)> {
        //self.continuation is a process
        let process = self.continuation;
        runtime.on_next_instant(Box::new(|run: &mut Runtime, _|
            process.call_mut(run, next.map(
            |(p, x): (P, P::Value)| (p.pause(), x)
            ))
        ))
    }
}

pub struct While<P> {
    process: P
}

impl<P, V> Process for While<P> where P: ProcessMut<Value = LoopStatus<V>>, V: 'static {
    type Value = V;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        self.process.call_mut(runtime, |runtime: &mut Runtime, (p, loop_status): (P, LoopStatus<V>)|
            match loop_status {
                LoopStatus::Continue => p.while_loop().call(runtime, next), //TODO better solution than calling while_loop
                LoopStatus::Exit(value) => return next.call(runtime, value)
            }
        );
    }
}

// ____                 ____  _                   _
//|  _ \ _   _ _ __ ___/ ___|(_) __ _ _ __   __ _| |
//| |_) | | | | '__/ _ \___ \| |/ _` | '_ \ / _` | |
//|  __/| |_| | | |  __/___) | | (_| | | | | (_| | |
//|_|    \__,_|_|  \___|____/|_|\__, |_| |_|\__,_|_|
//                              |___/

/// A shared pointer to a signal runtime.
#[derive(Clone)]
pub struct SignalRuntimeRef {
    runtime: Rc<SignalRuntime>,
}

/// Runtime for pure signals.
struct SignalRuntime {
    callbacks: Vec<Box<Continuation<()>>>,
}

impl SignalRuntime {
    fn add_callback<C>(&mut self, c: C) where C: Continuation<()> {
        self.callbacks.push(Box::new(c));
    }
}

impl SignalRuntimeRef {
    /// Sets the signal as emitted for the current instant.
    fn emit(self, runtime: &mut Runtime) {
        //self.runtime.;
    }

    /// Calls `c` at the first cycle where the signal is present.
    fn on_signal<C>(mut self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        if let Some(run) = Rc::get_mut(&mut self.runtime) {
            run.add_callback(c);
        }
    }

    // TODO: add other methods when needed.
}

/// A reactive signal.
pub trait Signal {
    /// Returns a reference to the signal's runtime.
    fn runtime(self) -> SignalRuntimeRef;

    /// Returns a process that waits for the next emission of the signal, current instant
    /// included.
    fn await_immediate(self) -> AwaitImmediate where Self: Sized {
        unimplemented!() // TODO
    }

    // TODO: add other methods if needed.
}

pub struct AwaitImmediate {
    // TODO
}

//impl Process for AwaitImmediate {
//    // TODO
//}
//
//impl ProcessMut for AwaitImmediate {
//    // TODO
//}

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
fn test_flatten() {
    let n = Rc::new(RefCell::new(0));
    let nn = n.clone();
    let mut runtime = Runtime::new();
    let p = value(value(42));

    assert_eq!(*n.borrow(), 0);
    p.flatten().call(&mut runtime, move |_: &mut Runtime, val| *nn.borrow_mut() = val);
    assert_eq!(*n.borrow(), 42);
}

#[test]
fn test_pause_process() {
    let n = Rc::new(RefCell::new(0));
    let nn = n.clone();
    let p = value(42).pause().map(move |val| {
        *nn.borrow_mut() = val;
    });

    assert_eq!(*n.borrow(), 0);
    execute_process(p);
    assert_eq!(*n.borrow(), 42);
}

#[test]
fn test_process_return() {
    assert_eq!(execute_process(value(42)), 42);
}

#[test]
fn test_process_while() {
    let n = Rc::new(RefCell::new(0));
    let nn = n.clone();

    let iter = move |_| {
        let mut x = nn.borrow_mut();
        *x = *x + 1;
        if *x == 42 {
            return LoopStatus::Exit(());
        } else {
            return LoopStatus::Continue;
        }
    };

    let p = value(()).map(
        iter
    ).while_loop();

    assert_eq!(*n.borrow(), 0);
    execute_process(p);
    assert_eq!(*n.borrow(), 42);
}

fn main() {
}
