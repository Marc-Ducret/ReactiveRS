use super::*;

// _   _       _                  ____  _                   _
//| | | |_ __ (_) __ _ _   _  ___/ ___|(_) __ _ _ __   __ _| |
//| | | | '_ \| |/ _` | | | |/ _ \___ \| |/ _` | '_ \ / _` | |
//| |_| | | | | | (_| | |_| |  __/___) | | (_| | | | | (_| | |
// \___/|_| |_|_|\__, |\__,_|\___|____/|_|\__, |_| |_|\__,_|_|
//                  |_|                   |___/

/// A shared pointer to a signal runtime.
pub struct USignalRuntimeRef<V, G> where V: Sized + 'static, G: Clone + 'static {
    signal_runtime: Arc<Mutex<USignalRuntime<V, G>>>,
}

impl<V, G> Clone for USignalRuntimeRef<V, G> where V: Sized + 'static, G: Clone + 'static {
    fn clone(&self) -> Self {
        USignalRuntimeRef{signal_runtime: self.signal_runtime.clone()}
    }
}

/// Runtime for pure signals.
struct USignalRuntime<V, G> where V: Sized + 'static, G: Clone + 'static {
    callbacks: Vec<Box<Continuation<()>>>,
    waiting_present: Vec<Box<Continuation<bool>>>,
    waiting_await: Vec<Box<Continuation<V>>>,
    status: bool,
    gather: Box<Fn(V, G) -> V>,
    default_value: V,
    current_value: V,
}

impl<V, G> USignalRuntime<V, G> where V: Sized + 'static, G: Clone + 'static {
    fn add_callback<C>(&mut self, c: C) where C: Continuation<()> {
        self.callbacks.push(Box::new(c));
    }
}

impl<V, G> USignalRuntimeRef<V, G> where V: Sized + 'static, G: Clone + 'static {
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
            runtime.on_end_of_instant(Box::new(move |runtime: &mut Runtime, ()| {
                let mut sig = sig_run.lock().unwrap();
                while let Some(c) = sig.waiting_await.pop() {
//                    let value = sig.current_value.clone();
//                    runtime.on_current_instant(Box::new(move |runtime: &mut Runtime, ()| {
//                       c.call_box(runtime, value);
//                    }));
                }
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

    fn await<C>(self, c: C) where C: Continuation<V> {
        let sig_ref = self.clone();
        let mut sig = sig_ref.signal_runtime.lock().unwrap();
        sig.waiting_await.push(Box::new(c));
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
pub trait USignal<V, G>: 'static where V: Sized + 'static, G: Clone + 'static {
    /// Returns a reference to the signal's runtime.
    fn runtime(&self) -> USignalRuntimeRef<V, G>;

    /// Returns a process that waits for the next emission of the signal, current instant
    /// included.
    fn await_immediate(&self) -> UAwaitImmediate<V, G> where Self: Sized {
        UAwaitImmediate {signal: self.runtime()}
    }

    fn await(&self) -> VAwait<V, G> where Self: Sized {
        VAwait {signal: self.runtime()}
    }

    fn emit(&self, value: G) -> UEmit<V, G> where Self: Sized {
        UEmit {signal: self.runtime(), value}
    }

    fn present(&self) -> UPresent<V, G> where Self: Sized {
        UPresent {signal: self.runtime()}
    }
}

pub struct UniqueSignal<V, G> where V: Sized + 'static, G: Clone + 'static {
    runtime: USignalRuntimeRef<V, G>
}

impl<V, G> UniqueSignal<V, G> where V: Sized + 'static, G: Clone + 'static {
    pub fn new(default_value: V, gather: Box<Fn(V, G) -> V>) -> UniqueSignal<V, G> {
        let runtime = USignalRuntime {
            status: false,
            callbacks: vec!(),
            waiting_present: vec!(),
            waiting_await: vec!(),
            current_value: default_value,
            default_value,
            gather,
        };
        UniqueSignal {
            runtime: USignalRuntimeRef {signal_runtime: Arc::new(Mutex::new(runtime))}
        }
    }
}

impl<V, G> Clone for UniqueSignal<V, G> where V: Sized + 'static, G: Clone + 'static {
    fn clone(&self) -> Self {
        UniqueSignal {runtime: self.runtime.clone()}
    }
}

impl<V, G> USignal<V, G> for UniqueSignal<V, G> where V: Sized + 'static, G: Clone + 'static {
    fn runtime(&self) -> USignalRuntimeRef<V, G> {
        self.runtime.clone()
    }
}

pub struct UAwaitImmediate<V, G> where V: Sized + 'static, G: Clone + 'static  {
    signal: USignalRuntimeRef<V, G>
}

impl<V, G> Process for UAwaitImmediate<V, G> where V: Sized + 'static, G: Clone + 'static {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        self.signal.on_signal(runtime, c);
    }
}

impl<V, G> ProcessMut for UAwaitImmediate<V, G> where V: Sized + 'static, G: Clone + 'static {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, ())> {
        let sig = self.signal.clone();
        self.signal.on_signal(runtime, |runtime: &mut Runtime, ()| {
            next.call(runtime, (UAwaitImmediate {signal: sig}, ()))
        });
    }
}

pub struct VAwait<V, G> where V: Sized + 'static, G: Clone + 'static  {
    signal: USignalRuntimeRef<V, G>
}

impl<V, G> Process for VAwait<V, G> where V: Sized + 'static, G: Clone + 'static {
    type Value = V;

    fn call<C>(self, _: &mut Runtime, c: C) where C: Continuation<V> {
        self.signal.await(c);
    }
}

impl<V, G> ProcessMut for VAwait<V, G> where V: Sized + 'static, G: Clone + 'static {
    fn call_mut<C>(self, _: &mut Runtime, next: C) where C: Continuation<(Self, V)> {
        let sig = self.signal.clone();
        self.signal.await(|runtime: &mut Runtime, v| {
            next.call(runtime, (VAwait {signal: sig}, v))
        });
    }
}

pub struct UEmit<V, G> where V: Sized + 'static, G: Clone + 'static {
    signal: USignalRuntimeRef<V, G>,
    value: G,
}

impl<V, G> Process for UEmit<V, G> where V: Sized + 'static, G: Clone + 'static {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        self.signal.emit(runtime, self.value);
        c.call(runtime, ());
    }
}

impl<V, G> ProcessMut for UEmit<V, G> where V: Sized + 'static, G: Clone + 'static {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, ())> {
        let sig = self.signal.clone();
        let val = self.value.clone();
        self.signal.emit(runtime, self.value);
        next.call(runtime, (UEmit {signal: sig, value: val}, ()))
    }
}

pub struct UPresent<V, G> where V: Sized + 'static, G: Clone + 'static {
    signal: USignalRuntimeRef<V, G>
}

impl<V, G> Process for UPresent<V, G> where V: Sized + 'static, G: Clone + 'static {
    type Value = bool;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<bool> {
        self.signal.test_present(runtime, next);
    }
}

impl<V, G> ProcessMut for UPresent<V, G> where V: Sized + 'static, G: Clone + 'static {

    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, bool)> {
        let sig = self.signal.clone();
        self.signal.test_present(runtime, move |runtime: &mut Runtime, status: bool| {
            next.call(runtime, (UPresent {signal: sig}, status))
        });
    }
}