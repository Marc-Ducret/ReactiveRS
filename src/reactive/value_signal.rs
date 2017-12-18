use super::*;

// __     __    _            ____  _                   _
// \ \   / /_ _| |_   _  ___/ ___|(_) __ _ _ __   __ _| |
//  \ \ / / _` | | | | |/ _ \___ \| |/ _` | '_ \ / _` | |
//   \ V / (_| | | |_| |  __/___) | | (_| | | | | (_| | |
//    \_/ \__,_|_|\__,_|\___|____/|_|\__, |_| |_|\__,_|_|
//                                   |___/

/// A shared pointer to a signal runtime.
#[derive(Clone)]
pub struct SignalRuntimeRef<V, G> where V: Copy + 'static, G: Copy + 'static {
    signal_runtime: Arc<Mutex<SignalRuntime<V, G>>>,
}

/// Runtime for pure signals.
struct SignalRuntime<V, G> where V: Copy + 'static, G: Copy + 'static {
    callbacks: Vec<Box<Continuation<()>>>,
    waiting_present: Vec<Box<Continuation<bool>>>,
    status: bool,
    gather: Box<Fn(V, G) -> V>,
    default_value: V,
    current_value: V,
}

impl<V, G> SignalRuntime<V, G> where V: Copy + 'static, G: Copy + 'static {
    fn add_callback<C>(&mut self, c: C) where C: Continuation<()> {
        self.callbacks.push(Box::new(c));
    }
}

impl<V, G> SignalRuntimeRef<V, G> where V: Copy + 'static, G: Copy + 'static {
    /// Sets the signal as emitted for the current instant.
    fn emit(self, runtime: &mut Runtime, value: G) {
        {
            let sig_run = self.signal_runtime.clone();
            let mut sig = sig_run.lock().unwrap();
            while let Some(c) = sig.callbacks.pop() {
                runtime.on_current_instant(c);
            }
            while let Some(c) = sig.waiting_present.pop() {
                runtime.on_current_instant(Box::new(|runtime: &mut Runtime, ()| c.call_box(runtime, true)));
            }
            sig.current_value = (sig.gather)(sig.current_value, value);
            sig.status = true;
        }

        {
            let sig_run = self.signal_runtime.clone();
            runtime.on_end_of_instant(Box::new(move |_: &mut Runtime, ()| {
                let mut sig = sig_run.lock().unwrap();
                sig.current_value = sig.default_value;
                sig.status = false;
            }))
        }
    }

    /// Calls `c` at the first cycle where the signal is present.
    fn on_signal<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        let sig_run = self.signal_runtime.clone();
        let mut sig = sig_run.lock().unwrap();
        if sig.status {
            runtime.on_current_instant(Box::new(c));
        } else {
            sig.add_callback(c);
        }
    }

    fn test_present<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<bool> {
        let sig_run = self.signal_runtime.clone();
        let mut sig = sig_run.lock().unwrap();
        if sig.status {
            c.call(runtime, true);
        } else {
            if sig.waiting_present.is_empty() {
                let sig_run = self.signal_runtime.clone();
                runtime.on_end_of_instant(Box::new(move |runtime: &mut Runtime, ()| {
                    let mut sig = sig_run.lock().unwrap();
                    while let Some(c) = sig.waiting_present.pop() {
                        c.call_box(runtime, false)
                    }
                }));
            }
            sig.waiting_present.push(Box::new(c));
        }
    }
}

/// A reactive signal.
pub trait Signal<V, G>: 'static where V: Copy + 'static, G: Copy + 'static {
    /// Returns a reference to the signal's runtime.
    fn runtime(&self) -> SignalRuntimeRef<V, G>;

    /// Returns a process that waits for the next emission of the signal, current instant
    /// included.
    fn await_immediate(&self) -> AwaitImmediate<V, G> where Self: Sized {
        AwaitImmediate { signal: self.runtime() }
    }

    fn emit(&self, value: G) -> Emit<V, G> where Self: Sized {
        Emit {signal: self.runtime(), value}
    }

    fn present(&self) -> Present<V, G> where Self: Sized {
        Present {signal: self.runtime()}
    }
}

pub struct ValueSignal<V, G> where V: Copy + 'static, G: Copy + 'static {
    runtime: SignalRuntimeRef<V, G>
}

impl<V, G> ValueSignal<V, G> where V: Copy + 'static, G: Copy + 'static {
    pub fn new(default_value: V, gather: Box<Fn(V, G) -> V>) -> ValueSignal<V, G> {
        let runtime = SignalRuntime {
            status: false,
            callbacks: vec!(),
            waiting_present: vec!(),
            current_value: default_value,
            default_value,
            gather,
        };
        ValueSignal {
            runtime: SignalRuntimeRef {signal_runtime: Arc::new(Mutex::new(runtime))}
        }
    }
}

impl<V, G> Clone for ValueSignal<V, G> where V: Copy + 'static, G: Copy + 'static {
    fn clone(&self) -> Self {
        ValueSignal {runtime: self.runtime.clone()}
    }
}

impl<V, G> Signal<V, G> for ValueSignal<V, G> where V: Copy + 'static, G: Copy + 'static {
    fn runtime(&self) -> SignalRuntimeRef<V, G> {
        self.runtime.clone()
    }
}

pub struct AwaitImmediate<V, G> where V: Copy + 'static, G: Copy + 'static  {
    signal: SignalRuntimeRef<V, G>
}

impl<V, G> Process for AwaitImmediate<V, G> where V: Copy + 'static, G: Copy + 'static {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        self.signal.on_signal(runtime, c);
    }
}

impl<V, G> ProcessMut for AwaitImmediate<V, G> where V: Copy + 'static, G: Copy + 'static {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, ())> {
        let sig = self.signal.clone();
        self.signal.on_signal(runtime, |runtime: &mut Runtime, ()| {
            next.call(runtime, ( AwaitImmediate {signal: sig}, ()))
        });
    }
}

pub struct Await<V, G> where V: Copy + 'static, G: Copy + 'static  {
    signal: SignalRuntimeRef<V, G>
}

impl<V, G> Process for Await<V, G> where V: Copy + 'static, G: Copy + 'static {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        self.signal.on_signal(runtime, c);
    }
}

impl<V, G> ProcessMut for Await<V, G> where V: Copy + 'static, G: Copy + 'static {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, ())> {
        let sig = self.signal.clone();
        self.signal.on_signal(runtime, |runtime: &mut Runtime, ()| {
            next.call(runtime, ( Await {signal: sig}, ()))
        });
    }
}

pub struct Emit<V, G> where V: Copy + 'static, G: Copy + 'static {
    signal: SignalRuntimeRef<V, G>,
    value: G,
}

impl<V, G> Process for Emit<V, G> where V: Copy + 'static, G: Copy + 'static {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        self.signal.emit(runtime, self.value);
        c.call(runtime, ());
    }
}

impl<V, G> ProcessMut for Emit<V, G> where V: Copy + 'static, G: Copy + 'static {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, ())> {
        let sig = self.signal.clone();
        let val = self.value.clone();
        self.signal.emit(runtime, self.value);
        next.call(runtime, (Emit {signal: sig, value: val}, ()))
    }
}

pub struct Present<V, G> where V: Copy + 'static, G: Copy + 'static {
    signal: SignalRuntimeRef<V, G>
}

impl<V, G> Process for Present<V, G> where V: Copy + 'static, G: Copy + 'static {
    type Value = bool;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<bool> {
        self.signal.test_present(runtime, next);
    }
}

impl<V, G> ProcessMut for Present<V, G> where V: Copy + 'static, G: Copy + 'static {

    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, bool)> {
        let sig = self.signal.clone();
        self.signal.test_present(runtime, move |runtime: &mut Runtime, status: bool| {
            next.call(runtime, (Present{signal: sig}, status))
        });
    }
}