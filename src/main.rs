use std::rc::Rc;
use std::cell::RefCell;
use std::cell::Cell;
use std::option::Option;


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

    fn join<P>(self, process: P) -> Join<Self, P> where Self: Sized, P: Process {
        Join {
            p1: self,
            p2: process
        }
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

pub struct Join<P1, P2> { p1: P1, p2: P2 }

impl<P1, P2> Process for Join<P1, P2> where P1: Process, P2: Process {
    type Value = (P1::Value, P2::Value);
    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        struct JoinPoint<T1, T2, C> {
            x1: Option<T1>,
            x2: Option<T2>,
            next: Option<C>
        }

        impl<T1, T2, C> JoinPoint<T1, T2, C> where C: Continuation<(T1, T2)> {
            fn call_continuation(&mut self, run: &mut Runtime) {
                if self.x1.is_some() {
                    if self.x2.is_some() {
                        let next = self.next.take();
                        let x1 = self.x1.take();
                        let x2 = self.x2.take();
                        if let Some(y1) = x1 {
                            if let Some(y2) = x2 {
                                if let Some(cont) = next {
                                    cont.call(run, (y1, y2));
                                }
                            }
                        }
                    }
                }
            }
        };

        let jp = Rc::new(RefCell::new(JoinPoint{x1: None, x2: None, next: Some(next)}));

        let jp1 = jp.clone();
        self.p1.call(runtime, move |run: &mut Runtime, x1| {
            jp1.borrow_mut().x1 = Some(x1);
            jp1.borrow_mut().call_continuation(run)
        });

        let jp2 = jp.clone();
        self.p2.call(runtime, move |run: &mut Runtime, x2| {
            jp2.borrow_mut().x2 = Some(x2);
            jp2.borrow_mut().call_continuation(run)
        });
    }
}

pub fn join<P1, P2>(p1: P1, p2: P2) -> Join<P1, P2> {
    Join {p1, p2}
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
    signal_runtime: Rc<SignalRuntime>,
}

/// Runtime for pure signals.
struct SignalRuntime {
    callbacks: Vec<Box<Continuation<()>>>,
    status: bool,
}

impl SignalRuntime {
    fn add_callback<C>(&mut self, c: C) where C: Continuation<()> {
        self.callbacks.push(Box::new(c));
    }
}

impl SignalRuntimeRef {
    /// Sets the signal as emitted for the current instant.
    fn emit(mut self, runtime: &mut Runtime) {
        if let Some(sig_run) = Rc::get_mut(&mut self.signal_runtime) {
            while let Some(c) = sig_run.callbacks.pop() {
                c.call_box(runtime, ());
            }
            sig_run.status = true;
        }
    }

    /// Calls `c` at the first cycle where the signal is present.
    fn on_signal<C>(mut self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        if let Some(sig_run) = Rc::get_mut(&mut self.signal_runtime) {
            if sig_run.status {
                c.call(runtime, ());
            } else {
                sig_run.add_callback(c);
            }
        }
    }

    // TODO: add other methods when needed.
}

/// A reactive signal.
pub trait Signal: 'static {
    /// Returns a reference to the signal's runtime.
    fn runtime(self) -> SignalRuntimeRef;

    /// Returns a process that waits for the next emission of the signal, current instant
    /// included.
    fn await_immediate(self) -> AwaitImmediate<Self> where Self: Sized {
        AwaitImmediate { signal: self }
    }

    // TODO: add other methods if needed.
}

pub struct AwaitImmediate<S> {
    signal: S
}

impl<S> Process for AwaitImmediate<S> where S: Signal{
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        self.signal.runtime().on_signal(runtime, c);
    }
}

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
fn test_join_process() {
    let n = Rc::new(RefCell::new((0, 0)));
    let nn = n.clone();
    let p = join(value(42), value(1337)).map(move |val| {
        *nn.borrow_mut() = val;
    });

    assert_eq!(*n.borrow(), (0, 0));
    execute_process(p);
    assert_eq!(*n.borrow(), (42, 1337));
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
