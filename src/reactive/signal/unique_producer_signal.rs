use super::*;

//  _   _ ____  ____  _                   _
// | | | |  _ \/ ___|(_) __ _ _ __   __ _| |
// | | | | |_) \___ \| |/ _` | '_ \ / _` | |
// | |_| |  __/ ___) | | (_| | | | | (_| | |
//  \___/|_|   |____/|_|\__, |_| |_|\__,_|_|
//                      |___/

pub struct UPSignalRuntimeRef<V> where V: Clone + Send + Sync + Sized + 'static {
    signal_runtime: Arc<Mutex<UPSignalRuntime<V>>>,
}

impl<V> Clone for UPSignalRuntimeRef<V> where V: Clone + Send + Sync + Sized + 'static {
    fn clone(&self) -> Self {
        UPSignalRuntimeRef {signal_runtime: self.signal_runtime.clone()}
    }
}

struct UPSignalRuntime<V> where V: Clone + Send + Sync + Sized + 'static {
    callbacks: Vec<Box<Continuation<V>>>,
    waiting_present: Vec<Box<Continuation<bool>>>,
    status: bool,
    default_value: V,
    current_value: V,
}

impl<V> UPSignalRuntime<V> where V: Clone + Send + Sync + Sized + 'static {
    fn add_callback<C>(&mut self, c: C) where C: Continuation<V> {
        self.callbacks.push(Box::new(c));
    }
}

impl<V> UPSignalRuntimeRef<V> where V: Clone + Send + Sync + Sized + 'static {
    fn emit(self, runtime: &mut Runtime, value: V) {
        {
            let sig_run = self.signal_runtime.clone();
            let mut sig = sig_run.lock().unwrap();
            sig.current_value = value;
            sig.status = true;
            while let Some(c) = sig.callbacks.pop() {
                let value = sig.current_value.clone();
                runtime.on_current_instant(Box::new(move |runtime: &mut Runtime, ()| c.call_box(runtime, value)));
            }
            while let Some(c) = sig.waiting_present.pop() {
                runtime.on_current_instant(Box::new(|runtime: &mut Runtime, ()| c.call_box(runtime, true)));
            }
        }

        {
            let sig_run = self.signal_runtime.clone();
            runtime.on_end_of_instant(Box::new(move |_: &mut Runtime, ()| {
                let mut sig = sig_run.lock().unwrap();
                sig.current_value = sig.default_value.clone();
                sig.status = false;
            }))
        }
    }

    fn on_signal<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<V> {
        let sig_run = self.signal_runtime.clone();
        let mut sig = sig_run.lock().unwrap();
        if sig.status {
            let value = sig.current_value.clone();
            runtime.on_current_instant(Box::new(move |runtime: &mut Runtime, ()| c.call(runtime, value)));
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

pub trait UPSignal<V>: 'static where V: Clone + Send + Sync + Sized + 'static {
    fn runtime(&self) -> UPSignalRuntimeRef<V>;

    fn emit<P>(self, value: P) -> UPEmit<V, P> where Self: Sized, P: Process<Value = V> {
        UPEmit {signal: self.runtime(), value}
    }
}

pub trait UPSignalConsumer<V>: 'static where V: Clone + Send + Sync + Sized + 'static {
    fn runtime(&self) -> UPSignalRuntimeRef<V>;

    fn await_immediate(&self) -> UPAwaitImmediate<V> where Self: Sized {
        UPAwaitImmediate {signal: self.runtime()}
    }

    fn present(&self) -> UPPresent<V> where Self: Sized {
        UPPresent {signal: self.runtime()}
    }
}

pub struct UniqueProducerSignalProducer<V> where V: Clone + Send + Sync + Sized + 'static {
    runtime: UPSignalRuntimeRef<V>
}

impl<V> UniqueProducerSignalProducer<V> where V: Clone + Send + Sync + Sized + 'static {
    pub fn new(default_value: V) -> (UniqueProducerSignalProducer<V>, UniqueProducerSignalConsumer<V>) {
        let runtime = UPSignalRuntime {
            status: false,
            callbacks: vec!(),
            waiting_present: vec!(),
            current_value: default_value.clone(),
            default_value,
        };
        let signal_run = UPSignalRuntimeRef {signal_runtime: Arc::new(Mutex::new(runtime))};
        (
            UniqueProducerSignalProducer {
                runtime: signal_run.clone()
            },
            UniqueProducerSignalConsumer {
                runtime: signal_run.clone()
            }
        )
    }
}

impl<V> UPSignal<V> for UniqueProducerSignalProducer<V> where V: Clone + Send + Sync + Sized + 'static {
    fn runtime(&self) -> UPSignalRuntimeRef<V> {
        self.runtime.clone()
    }
}

pub struct UniqueProducerSignalConsumer<V> where V: Clone + Send + Sync + Sized + 'static {
    runtime: UPSignalRuntimeRef<V>
}

impl<V> UPSignalConsumer<V> for UniqueProducerSignalConsumer<V> where V: Clone + Send + Sync + Sized + 'static {
    fn runtime(&self) -> UPSignalRuntimeRef<V> {
        self.runtime.clone()
    }
}


impl<V> Clone for UniqueProducerSignalConsumer<V> where V: Clone + Send + Sync + Sized + 'static {
    fn clone(&self) -> Self {
        UniqueProducerSignalConsumer {runtime: self.runtime.clone()}
    }
}

pub struct UPAwaitImmediate<V> where V: Clone + Send + Sync + Sized + 'static  {
    signal: UPSignalRuntimeRef<V>
}

impl<V> Process for UPAwaitImmediate<V> where V: Clone + Send + Sync + Sized + 'static {
    type Value = V;

    fn call<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<V> {
        self.signal.on_signal(runtime, c);
    }
}

impl<V> ProcessMut for UPAwaitImmediate<V> where V: Clone + Send + Sync + Sized + 'static {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, V)> {
        let sig = self.signal.clone();
        self.signal.on_signal(runtime, |runtime: &mut Runtime, v| {
            next.call(runtime, (UPAwaitImmediate {signal: sig}, v))
        });
    }
}

pub struct UPEmit<V, P> where V: Clone + Send + Sync + Sized + 'static, P: Process<Value = V> {
    signal: UPSignalRuntimeRef<V>,
    value: P,
}

impl<V, P> Process for UPEmit<V, P> where V: Clone + Send + Sync + Sized + 'static, P: Process<Value = V> {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        let sig = self.signal.clone();

        self.value.call(runtime, move |runtime: &mut Runtime, v| {
            sig.emit(runtime, v);
            c.call(runtime, ());
        });
    }
}

impl<V, P> ProcessMut for UPEmit<V, P> where V: Clone + Send + Sync + Sized + 'static, P: ProcessMut<Value = V> {
    fn call_mut<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<(Self, ())> {
        let sig = self.signal.clone();

        self.value.call_mut(runtime, move |runtime: &mut Runtime, (process, v)| {
            sig.clone().emit(runtime, v);
            c.call(runtime, (UPEmit {signal: sig, value: process}, ()));
        });
    }
}

pub struct UPPresent<V> where V: Clone + Send + Sync + Sized + 'static {
    signal: UPSignalRuntimeRef<V>
}

impl<V> Process for UPPresent<V> where V: Clone + Send + Sync + Sized + 'static {
    type Value = bool;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<bool> {
        self.signal.test_present(runtime, next);
    }
}

impl<V> ProcessMut for UPPresent<V> where V: Clone + Send + Sync + Sized + 'static {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, bool)> {
        let sig = self.signal.clone();
        self.signal.test_present(runtime, move |runtime: &mut Runtime, status: bool| {
            next.call(runtime, (UPPresent {signal: sig}, status))
        });
    }
}