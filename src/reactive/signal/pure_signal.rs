use super::*;

// ____                 ____  _                   _
//|  _ \ _   _ _ __ ___/ ___|(_) __ _ _ __   __ _| |
//| |_) | | | | '__/ _ \___ \| |/ _` | '_ \ / _` | |
//|  __/| |_| | | |  __/___) | | (_| | | | | (_| | |
//|_|    \__,_|_|  \___|____/|_|\__, |_| |_|\__,_|_|
//                              |___/

#[derive(Clone)]
pub struct PSignalRuntimeRef {
    pub signal_runtime: Arc<Mutex<PSignalRuntime>>,
}

pub struct PSignalRuntime {
    callbacks: Vec<Box<Continuation<()>>>,
    waiting_present: Vec<Box<Continuation<bool>>>,
    pub status: bool,
}

impl PSignalRuntime {
    fn add_callback<C>(&mut self, c: C) where C: Continuation<()> {
        self.callbacks.push(Box::new(c));
    }
}

impl PSignalRuntimeRef {
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

pub trait PSignal: 'static {
    fn runtime(&self) -> PSignalRuntimeRef;

    fn await_immediate(&self) -> PAwaitImmediate where Self: Sized {
        PAwaitImmediate { signal: self.runtime() }
    }

    fn emit(&self) -> PEmit where Self: Sized {
        PEmit {signal: self.runtime()}
    }

    fn present(&self) -> PPresent where Self: Sized {
        PPresent {signal: self.runtime()}
    }
}

pub struct PureSignal {
    runtime: PSignalRuntimeRef
}

impl PureSignal {
    pub fn new() -> PureSignal {
        let runtime = PSignalRuntime {status: false, callbacks: vec!(), waiting_present: vec!()};
        PureSignal {
            runtime: PSignalRuntimeRef {signal_runtime: Arc::new(Mutex::new(runtime))}
        }
    }
}

impl Clone for PureSignal {
    fn clone(&self) -> Self {
        PureSignal {runtime: self.runtime.clone()}
    }
}

impl PSignal for PureSignal {
    fn runtime(&self) -> PSignalRuntimeRef {
        self.runtime.clone()
    }
}

pub struct PAwaitImmediate {
    signal: PSignalRuntimeRef
}

impl Process for PAwaitImmediate {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        self.signal.on_signal(runtime, c);
    }
}

impl ProcessMut for PAwaitImmediate {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, ())> {
        let sig = self.signal.clone();
        self.signal.on_signal(runtime, |runtime: &mut Runtime, ()| {
            next.call(runtime, (PAwaitImmediate {signal: sig}, ()))
        });
    }
}

pub struct PEmit {
    signal: PSignalRuntimeRef
}

impl Process for PEmit {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        self.signal.emit(runtime);
        c.call(runtime, ());
    }
}

impl ProcessMut for PEmit {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, ())> {
        let sig = self.signal.clone();
        self.signal.emit(runtime);
        next.call(runtime, (PEmit {signal: sig}, ()))
    }
}

pub struct PPresent {
    signal: PSignalRuntimeRef
}

impl Process for PPresent {
    type Value = bool;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<bool> {
        self.signal.test_present(runtime, next);
    }
}

impl ProcessMut for PPresent {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, bool)> {
        let sig = self.signal.clone();
        self.signal.test_present(runtime, move |runtime: &mut Runtime, status: bool| {
            next.call(runtime, (PPresent {signal: sig}, status))
        });
    }
}