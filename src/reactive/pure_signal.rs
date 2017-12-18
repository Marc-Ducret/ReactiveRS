use super::*;

// ____                 ____  _                   _
//|  _ \ _   _ _ __ ___/ ___|(_) __ _ _ __   __ _| |
//| |_) | | | | '__/ _ \___ \| |/ _` | '_ \ / _` | |
//|  __/| |_| | | |  __/___) | | (_| | | | | (_| | |
//|_|    \__,_|_|  \___|____/|_|\__, |_| |_|\__,_|_|
//                              |___/

/// A shared pointer to a signal runtime.
#[derive(Clone)]
pub struct SignalRuntimeRef {
    pub signal_runtime: Arc<Mutex<SignalRuntime>>,
}

/// Runtime for pure signals.
pub struct SignalRuntime {
    callbacks: Vec<Box<Continuation<()>>>,
    waiting_present: Vec<Box<Continuation<bool>>>,
    pub status: bool,
}

impl SignalRuntime {
    fn add_callback<C>(&mut self, c: C) where C: Continuation<()> {
        self.callbacks.push(Box::new(c));
    }
}

impl SignalRuntimeRef {
    /// Sets the signal as emitted for the current instant.
    fn emit(self, runtime: &mut Runtime) {
        {
            let sig_run = self.signal_runtime.clone();
            let mut sig = sig_run.lock().unwrap();
            while let Some(c) = sig.callbacks.pop() {
                runtime.on_current_instant(c);
            }
            while let Some(c) = sig.waiting_present.pop() {
                runtime.on_current_instant(Box::new(|runtime: &mut Runtime, ()| c.call_box(runtime, true)));
            }
            sig.status = true;
        }

        {
            let sig_run = self.signal_runtime.clone();
            runtime.on_end_of_instant(Box::new(move |_: &mut Runtime, ()| {
                let mut sig = sig_run.lock().unwrap();
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
pub trait Signal: 'static {
    /// Returns a reference to the signal's runtime.
    fn runtime(&self) -> SignalRuntimeRef;

    /// Returns a process that waits for the next emission of the signal, current instant
    /// included.
    fn await_immediate(&self) -> AwaitImmediate where Self: Sized {
        AwaitImmediate { signal: self.runtime() }
    }

    fn emit(&self) -> Emit where Self: Sized {
        Emit {signal: self.runtime()}
    }

    fn present(&self) -> Present where Self: Sized {
        Present {signal: self.runtime()}
    }
}

pub struct PureSignal {
    runtime: SignalRuntimeRef
}

impl PureSignal {
    pub fn new() -> PureSignal {
        let runtime = SignalRuntime {status: false, callbacks: vec!(), waiting_present: vec!()};
        PureSignal {
            runtime: SignalRuntimeRef {signal_runtime: Arc::new(Mutex::new(runtime))}
        }
    }
}

impl Clone for PureSignal {
    fn clone(&self) -> Self {
        PureSignal {runtime: self.runtime.clone()}
    }
}

impl Signal for PureSignal {
    fn runtime(&self) -> SignalRuntimeRef {
        self.runtime.clone()
    }
}

pub struct AwaitImmediate {
    signal: SignalRuntimeRef
}

impl Process for AwaitImmediate {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        self.signal.on_signal(runtime, c);
    }
}

impl ProcessMut for AwaitImmediate {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, ())> {
        let sig = self.signal.clone();
        self.signal.on_signal(runtime, |runtime: &mut Runtime, ()| {
            next.call(runtime, ( AwaitImmediate {signal: sig}, ()))
        });
    }
}

pub struct Emit {
    signal: SignalRuntimeRef
}

impl Process for Emit {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        self.signal.emit(runtime);
        c.call(runtime, ());
    }
}

impl ProcessMut for Emit {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, ())> {
        let sig = self.signal.clone();
        self.signal.emit(runtime);
        next.call(runtime, (Emit {signal: sig}, ()))
    }
}

pub struct Present {
    signal: SignalRuntimeRef
}

impl Process for Present {
    type Value = bool;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<bool> {
        self.signal.test_present(runtime, next);
    }
}

impl ProcessMut for Present {

    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, bool)> {
        let sig = self.signal.clone();
        self.signal.test_present(runtime, move |runtime: &mut Runtime, status: bool| {
            next.call(runtime, (Present{signal: sig}, status))
        });
    }
}