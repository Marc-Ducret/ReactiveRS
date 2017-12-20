use super::*;

// _   _       _                  ____  _                   _
//| | | |_ __ (_) __ _ _   _  ___/ ___|(_) __ _ _ __   __ _| |
//| | | | '_ \| |/ _` | | | |/ _ \___ \| |/ _` | '_ \ / _` | |
//| |_| | | | | | (_| | |_| |  __/___) | | (_| | | | | (_| | |
// \___/|_| |_|_|\__, |\__,_|\___|____/|_|\__, |_| |_|\__,_|_|
//                  |_|                   |___/

pub struct USignalRuntimeRef<V, G> where V: Sized + 'static, G: 'static {
    signal_runtime: Arc<Mutex<USignalRuntime<V, G>>>,
}

impl<V, G> Clone for USignalRuntimeRef<V, G> where V: Sized + 'static, G: 'static {
    fn clone(&self) -> Self {
        USignalRuntimeRef{signal_runtime: self.signal_runtime.clone()}
    }
}

struct USignalRuntime<V, G> where V: Sized + 'static, G: 'static {
    callbacks: Vec<Box<Continuation<()>>>,
    waiting_present: Vec<Box<Continuation<bool>>>,
    waiting_await: Option<Box<Continuation<V>>>,
    status: bool,
    gather: Box<Fn(V, G) -> V>,
    default_value: Box<Fn() -> V>,
    current_value: V,
}

impl<V, G> USignalRuntime<V, G> where V: Sized + 'static, G: 'static {
    fn add_callback<C>(&mut self, c: C) where C: Continuation<()> {
        self.callbacks.push(Box::new(c));
    }
}

impl<V, G> USignalRuntimeRef<V, G> where V: Sized + 'static, G: 'static {
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
            let mut prev_value = (sig.default_value)();
            std::mem::swap(&mut prev_value, &mut sig.current_value);
            sig.current_value = (sig.gather)(prev_value, value);
            sig.status = true;
        }

        {
            let sig_run = self.signal_runtime.clone();
            runtime.on_end_of_instant(Box::new(move |runtime: &mut Runtime, ()| {
                let mut sig = sig_run.lock().unwrap();
                let mut prev_value = (sig.default_value)();
                std::mem::swap(&mut prev_value, &mut sig.current_value);
                let mut waiting: Option<Box<Continuation<V>>> = None;
                std::mem::swap(&mut waiting, &mut sig.waiting_await);
                if let Some(c) = waiting {
                    runtime.on_current_instant(Box::new(move |runtime: &mut Runtime, ()| {
                        c.call_box(runtime, prev_value);
                    }));
                }
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

    fn await<C>(self, c: C) where C: Continuation<V> {
        let sig_ref = self.clone();
        let mut sig = sig_ref.signal_runtime.lock().unwrap();
        if let Some(_) = sig.waiting_await {
            unreachable!();
        }
        sig.waiting_await = Some(Box::new(c));
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

pub trait USignal<V, G>: 'static where V: Sized + 'static, G: 'static {
    fn runtime(&self) -> USignalRuntimeRef<V, G>;

    fn await_immediate(&self) -> UAwaitImmediate<V, G> where Self: Sized {
        UAwaitImmediate {signal: self.runtime()}
    }

    fn emit<P>(&self, value: P) -> UEmit<V, G, P> where Self: Sized, P: Process<Value = G> {
        UEmit {signal: self.runtime(), value}
    }

    fn present(&self) -> UPresent<V, G> where Self: Sized {
        UPresent {signal: self.runtime()}
    }
}

pub trait USignalConsumer<V, G>: 'static where V: Sized + 'static, G: 'static {
    fn runtime(&self) -> USignalRuntimeRef<V, G>;

    fn await(self) -> VAwait<V, G> where Self: Sized {
        VAwait {signal: self.runtime()}
    }
}

pub struct UniqueSignal<V, G> where V: Sized + 'static, G: 'static {
    runtime: USignalRuntimeRef<V, G>
}

impl<V, G> UniqueSignal<V, G> where V: Sized + 'static, G: 'static {
    pub fn new(default_value: Box<Fn() -> V>, gather: Box<Fn(V, G) -> V>) -> (UniqueSignal<V, G>, UniqueSignalConsumer<V, G>) {
        let runtime = USignalRuntime {
            status: false,
            callbacks: vec!(),
            waiting_present: vec!(),
            waiting_await: None,
            current_value: default_value(),
            default_value,
            gather,
        };
        let signal_run = USignalRuntimeRef {signal_runtime: Arc::new(Mutex::new(runtime))};
        (
            UniqueSignal {
                runtime: signal_run.clone()
            },
            UniqueSignalConsumer {
                runtime: signal_run.clone()
            }
        )
    }
}

impl<V, G> Clone for UniqueSignal<V, G> where V: Sized + 'static, G: 'static {
    fn clone(&self) -> Self {
        UniqueSignal {runtime: self.runtime.clone()}
    }
}

impl<V, G> USignal<V, G> for UniqueSignal<V, G> where V: Sized + 'static, G: 'static {
    fn runtime(&self) -> USignalRuntimeRef<V, G> {
        self.runtime.clone()
    }
}

pub struct UniqueSignalConsumer<V, G> where V: Sized + 'static, G: 'static {
    runtime: USignalRuntimeRef<V, G>
}

impl<V, G> USignalConsumer<V, G> for UniqueSignalConsumer<V, G> where V: Sized + 'static, G: 'static {
    fn runtime(&self) -> USignalRuntimeRef<V, G> {
        self.runtime.clone()
    }
}

pub struct UAwaitImmediate<V, G> where V: Sized + 'static, G: 'static  {
    signal: USignalRuntimeRef<V, G>
}

impl<V, G> Process for UAwaitImmediate<V, G> where V: Sized + 'static, G: 'static {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        self.signal.on_signal(runtime, c);
    }
}

impl<V, G> ProcessMut for UAwaitImmediate<V, G> where V: Sized + 'static, G: 'static {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, ())> {
        let sig = self.signal.clone();
        self.signal.on_signal(runtime, |runtime: &mut Runtime, ()| {
            next.call(runtime, (UAwaitImmediate {signal: sig}, ()))
        });
    }
}

pub struct VAwait<V, G> where V: Sized + 'static, G: 'static  {
    signal: USignalRuntimeRef<V, G>
}

impl<V, G> Process for VAwait<V, G> where V: Sized + 'static, G: 'static {
    type Value = V;

    fn call<C>(self, _: &mut Runtime, c: C) where C: Continuation<V> {
        self.signal.await(c);
    }
}

impl<V, G> ProcessMut for VAwait<V, G> where V: Sized + 'static, G: 'static {
    fn call_mut<C>(self, _: &mut Runtime, next: C) where C: Continuation<(Self, V)> {
        let sig = self.signal.clone();
        self.signal.await(|runtime: &mut Runtime, v| {
            next.call(runtime, (VAwait {signal: sig}, v))
        });
    }
}

pub struct UEmit<V, G, P> where V: Sized + 'static, G: 'static, P: Process<Value = G> {
    signal: USignalRuntimeRef<V, G>,
    value: P,
}

impl<V, G, P> Process for UEmit<V, G, P> where V: Sized + 'static, G: 'static, P: Process<Value = G> {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        let sig = self.signal.clone();

        self.value.call(runtime, move |runtime: &mut Runtime, v| {
            sig.emit(runtime, v);
            c.call(runtime, ());
        });
    }
}

impl<V, G, P> ProcessMut for UEmit<V, G, P> where V: Sized + 'static, G: 'static, P: ProcessMut<Value = G> {
    fn call_mut<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<(Self, ())> {
        let sig = self.signal.clone();

        self.value.call_mut(runtime, move |runtime: &mut Runtime, (process, v)| {
            sig.clone().emit(runtime, v);
            c.call(runtime, (UEmit {signal: sig, value: process}, ()));
        });
    }
}

pub struct UPresent<V, G> where V: Sized + 'static, G: 'static {
    signal: USignalRuntimeRef<V, G>
}

impl<V, G> Process for UPresent<V, G> where V: Sized + 'static, G: 'static {
    type Value = bool;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<bool> {
        self.signal.test_present(runtime, next);
    }
}

impl<V, G> ProcessMut for UPresent<V, G> where V: Sized + 'static, G: 'static {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, bool)> {
        let sig = self.signal.clone();
        self.signal.test_present(runtime, move |runtime: &mut Runtime, status: bool| {
            next.call(runtime, (UPresent {signal: sig}, status))
        });
    }
}