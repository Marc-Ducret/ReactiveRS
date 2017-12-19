use super::*;

// __     __    _            ____  _                   _
// \ \   / /_ _| |_   _  ___/ ___|(_) __ _ _ __   __ _| |
//  \ \ / / _` | | | | |/ _ \___ \| |/ _` | '_ \ / _` | |
//   \ V / (_| | | |_| |  __/___) | | (_| | | | | (_| | |
//    \_/ \__,_|_|\__,_|\___|____/|_|\__, |_| |_|\__,_|_|
//                                   |___/

pub struct VSignalRuntimeRef<V, G> where V: Clone + 'static, G: 'static {
    signal_runtime: Arc<Mutex<VSignalRuntime<V, G>>>,
}

impl<V, G> Clone for VSignalRuntimeRef<V, G> where V: Clone + 'static, G: 'static {
    fn clone(&self) -> Self {
        VSignalRuntimeRef{signal_runtime: self.signal_runtime.clone()}
    }
}

struct VSignalRuntime<V, G> where V: Clone + 'static, G: 'static {
    callbacks: Vec<Box<Continuation<()>>>,
    waiting_present: Vec<Box<Continuation<bool>>>,
    waiting_await: Vec<Box<Continuation<V>>>,
    status: bool,
    gather: Box<Fn(V, G) -> V>,
    default_value: V,
    current_value: V,
}

impl<V, G> VSignalRuntime<V, G> where V: Clone + 'static, G: 'static {
    fn add_callback<C>(&mut self, c: C) where C: Continuation<()> {
        self.callbacks.push(Box::new(c));
    }
}

impl<V, G> VSignalRuntimeRef<V, G> where V: Clone + 'static, G: 'static {
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
            sig.current_value = (sig.gather)(sig.current_value.clone(), value);
            sig.status = true;
        }

        {
            let sig_run = self.signal_runtime.clone();
            runtime.on_end_of_instant(Box::new(move |runtime: &mut Runtime, ()| {
                let mut sig = sig_run.lock().unwrap();
                while let Some(c) = sig.waiting_await.pop() {
                    let value = sig.current_value.clone();
                    runtime.on_current_instant(Box::new(move |runtime: &mut Runtime, ()| {
                       c.call_box(runtime, value);
                    }));
                }
                sig.current_value = sig.default_value.clone();
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

pub trait VSignal<V, G>: 'static where V: Clone + 'static, G: 'static {
    fn runtime(&self) -> VSignalRuntimeRef<V, G>;

    fn await_immediate(&self) -> VAwaitImmediate<V, G> where Self: Sized {
        VAwaitImmediate {signal: self.runtime()}
    }

    fn await(&self) -> VAwait<V, G> where Self: Sized {
        VAwait {signal: self.runtime()}
    }

    fn emit<P>(&self, value: P) -> VEmit<V, G, P> where Self: Sized, P: Process<Value = G> {
        VEmit {signal: self.runtime(), value}
    }

    fn present(&self) -> VPresent<V, G> where Self: Sized {
        VPresent {signal: self.runtime()}
    }
}

pub struct ValueSignal<V, G> where V: Clone + 'static, G: 'static {
    runtime: VSignalRuntimeRef<V, G>
}

impl<V, G> ValueSignal<V, G> where V: Clone + 'static, G: 'static {
    pub fn new(default_value: V, gather: Box<Fn(V, G) -> V>) -> ValueSignal<V, G> {
        let runtime = VSignalRuntime {
            status: false,
            callbacks: vec!(),
            waiting_present: vec!(),
            waiting_await: vec!(),
            current_value: default_value.clone(),
            default_value,
            gather,
        };
        ValueSignal {
            runtime: VSignalRuntimeRef {signal_runtime: Arc::new(Mutex::new(runtime))}
        }
    }
}

impl<V, G> Clone for ValueSignal<V, G> where V: Clone + 'static, G: 'static {
    fn clone(&self) -> Self {
        ValueSignal {runtime: self.runtime.clone()}
    }
}

impl<V, G> VSignal<V, G> for ValueSignal<V, G> where V: Clone + 'static, G: 'static {
    fn runtime(&self) -> VSignalRuntimeRef<V, G> {
        self.runtime.clone()
    }
}

pub struct VAwaitImmediate<V, G> where V: Clone + 'static, G: 'static  {
    signal: VSignalRuntimeRef<V, G>
}

impl<V, G> Process for VAwaitImmediate<V, G> where V: Clone + 'static, G: 'static {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        self.signal.on_signal(runtime, c);
    }
}

impl<V, G> ProcessMut for VAwaitImmediate<V, G> where V: Clone + 'static, G: 'static {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, ())> {
        let sig = self.signal.clone();
        self.signal.on_signal(runtime, |runtime: &mut Runtime, ()| {
            next.call(runtime, (VAwaitImmediate {signal: sig}, ()))
        });
    }
}

pub struct VAwait<V, G> where V: Clone + 'static, G: 'static  {
    signal: VSignalRuntimeRef<V, G>
}

impl<V, G> Process for VAwait<V, G> where V: Clone + 'static, G: 'static {
    type Value = V;

    fn call<C>(self, _: &mut Runtime, c: C) where C: Continuation<V> {
        self.signal.await(c);
    }
}

impl<V, G> ProcessMut for VAwait<V, G> where V: Clone + 'static, G: 'static {
    fn call_mut<C>(self, _: &mut Runtime, next: C) where C: Continuation<(Self, V)> {
        let sig = self.signal.clone();
        self.signal.await(|runtime: &mut Runtime, v| {
            next.call(runtime, (VAwait {signal: sig}, v))
        });
    }
}

pub struct VEmit<V, G, P> where V: Clone + 'static, G: 'static, P: Process<Value = G> {
    signal: VSignalRuntimeRef<V, G>,
    value: P,
}

impl<V, G, P> Process for VEmit<V, G, P> where V: Clone + 'static, G: 'static, P: Process<Value = G> {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        let sig = self.signal.clone();

        self.value.call(runtime, move |runtime: &mut Runtime, v| {
            sig.emit(runtime, v);
            c.call(runtime, ());
        });
    }
}

impl<V, G, P> ProcessMut for VEmit<V, G, P> where V: Clone + 'static, G: 'static, P: ProcessMut<Value = G> {
    fn call_mut<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<(Self, ())> {
        let sig = self.signal.clone();

        self.value.call_mut(runtime, move |runtime: &mut Runtime, (process, v)| {
            sig.clone().emit(runtime, v);
            c.call(runtime, (VEmit {signal: sig, value: process}, ()));
        });
    }
}

pub struct VPresent<V, G> where V: Clone + 'static, G: 'static {
    signal: VSignalRuntimeRef<V, G>
}

impl<V, G> Process for VPresent<V, G> where V: Clone + 'static, G: 'static {
    type Value = bool;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<bool> {
        self.signal.test_present(runtime, next);
    }
}

impl<V, G> ProcessMut for VPresent<V, G> where V: Clone + 'static, G: 'static {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, bool)> {
        let sig = self.signal.clone();
        self.signal.test_present(runtime, move |runtime: &mut Runtime, status: bool| {
            next.call(runtime, (VPresent {signal: sig}, status))
        });
    }
}