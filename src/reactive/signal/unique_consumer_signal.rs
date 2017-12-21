use super::*;

//  _   _  ____ ____  _                   _
// | | | |/ ___/ ___|(_) __ _ _ __   __ _| |
// | | | | |   \___ \| |/ _` | '_ \ / _` | |
// | |_| | |___ ___) | | (_| | | | | (_| | |
//  \___/ \____|____/|_|\__, |_| |_|\__,_|_|
//                      |___/

pub struct UCSignalRuntimeRef<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
    signal_runtime: Arc<Mutex<UCSignalRuntime<V, G>>>,
}

impl<V, G> Clone for UCSignalRuntimeRef<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
    fn clone(&self) -> Self {
        UCSignalRuntimeRef{signal_runtime: self.signal_runtime.clone()}
    }
}

struct UCSignalRuntime<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
    callbacks: Vec<Box<Continuation<()>>>,
    waiting_present: Vec<Box<Continuation<bool>>>,
    waiting_await: Option<Box<Continuation<V>>>,
    status: bool,
    gather: Box<Fn(V, G) -> V + Send + Sync>,
    default_value: Box<Fn() -> V + Send + Sync>,
    current_value: V,
}

impl<V, G> UCSignalRuntime<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
    fn add_callback<C>(&mut self, c: C) where C: Continuation<()> {
        self.callbacks.push(Box::new(c));
    }
}

impl<V, G> UCSignalRuntimeRef<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
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

pub trait UCSignal<V, G>: 'static where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
    fn runtime(&self) -> UCSignalRuntimeRef<V, G>;

    fn await_immediate(&self) -> UCAwaitImmediate<V, G> where Self: Sized {
        UCAwaitImmediate {signal: self.runtime()}
    }

    fn emit<P>(&self, value: P) -> UCEmit<V, G, P> where Self: Sized, P: Process<Value = G> {
        UCEmit {signal: self.runtime(), value}
    }

    fn present(&self) -> UCPresent<V, G> where Self: Sized {
        UCPresent {signal: self.runtime()}
    }
}

pub trait UCSignalConsumer<V, G>: 'static where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
    fn runtime(&self) -> UCSignalRuntimeRef<V, G>;

    fn await(self) -> UCAwait<V, G> where Self: Sized {
        UCAwait {signal: self.runtime()}
    }
}

pub struct UniqueConsumerSignalProducer<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
    runtime: UCSignalRuntimeRef<V, G>
}

impl<V, G> UniqueConsumerSignalProducer<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
    pub fn new(default_value: Box<Fn() -> V + Send + Sync>, gather: Box<Fn(V, G) -> V + Send + Sync>) -> (UniqueConsumerSignalProducer<V, G>, UniqueConsumerSignalConsumer<V, G>) {
        let runtime = UCSignalRuntime {
            status: false,
            callbacks: vec!(),
            waiting_present: vec!(),
            waiting_await: None,
            current_value: default_value(),
            default_value,
            gather,
        };
        let signal_run = UCSignalRuntimeRef {signal_runtime: Arc::new(Mutex::new(runtime))};
        (
            UniqueConsumerSignalProducer {
                runtime: signal_run.clone()
            },
            UniqueConsumerSignalConsumer {
                runtime: signal_run.clone()
            }
        )
    }
}

impl<V, G> Clone for UniqueConsumerSignalProducer<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
    fn clone(&self) -> Self {
        UniqueConsumerSignalProducer {runtime: self.runtime.clone()}
    }
}

impl<V, G> UCSignal<V, G> for UniqueConsumerSignalProducer<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
    fn runtime(&self) -> UCSignalRuntimeRef<V, G> {
        self.runtime.clone()
    }
}

pub struct UniqueConsumerSignalConsumer<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
    runtime: UCSignalRuntimeRef<V, G>
}

impl<V, G> UCSignalConsumer<V, G> for UniqueConsumerSignalConsumer<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
    fn runtime(&self) -> UCSignalRuntimeRef<V, G> {
        self.runtime.clone()
    }
}

pub struct UCAwaitImmediate<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync  {
    signal: UCSignalRuntimeRef<V, G>
}

impl<V, G> Process for UCAwaitImmediate<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        self.signal.on_signal(runtime, c);
    }
}

impl<V, G> ProcessMut for UCAwaitImmediate<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, ())> {
        let sig = self.signal.clone();
        self.signal.on_signal(runtime, |runtime: &mut Runtime, ()| {
            next.call(runtime, (UCAwaitImmediate {signal: sig}, ()))
        });
    }
}

pub struct UCAwait<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync  {
    signal: UCSignalRuntimeRef<V, G>
}

impl<V, G> Process for UCAwait<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
    type Value = V;

    fn call<C>(self, _: &mut Runtime, c: C) where C: Continuation<V> {
        self.signal.await(c);
    }
}

impl<V, G> ProcessMut for UCAwait<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
    fn call_mut<C>(self, _: &mut Runtime, next: C) where C: Continuation<(Self, V)> {
        let sig = self.signal.clone();
        self.signal.await(|runtime: &mut Runtime, v| {
            next.call(runtime, (UCAwait {signal: sig}, v))
        });
    }
}

pub struct UCEmit<V, G, P> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync, P: Process<Value = G> {
    signal: UCSignalRuntimeRef<V, G>,
    value: P,
}

impl<V, G, P> Process for UCEmit<V, G, P> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync, P: Process<Value = G> {
    type Value = ();

    fn call<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<()> {
        let sig = self.signal.clone();

        self.value.call(runtime, move |runtime: &mut Runtime, v| {
            sig.emit(runtime, v);
            c.call(runtime, ());
        });
    }
}

impl<V, G, P> ProcessMut for UCEmit<V, G, P> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync, P: ProcessMut<Value = G> {
    fn call_mut<C>(self, runtime: &mut Runtime, c: C) where C: Continuation<(Self, ())> {
        let sig = self.signal.clone();

        self.value.call_mut(runtime, move |runtime: &mut Runtime, (process, v)| {
            sig.clone().emit(runtime, v);
            c.call(runtime, (UCEmit {signal: sig, value: process}, ()));
        });
    }
}

pub struct UCPresent<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
    signal: UCSignalRuntimeRef<V, G>
}

impl<V, G> Process for UCPresent<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
    type Value = bool;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<bool> {
        self.signal.test_present(runtime, next);
    }
}

impl<V, G> ProcessMut for UCPresent<V, G> where V: Sized + Send + Sync + 'static, G: 'static + Send + Sync {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, bool)> {
        let sig = self.signal.clone();
        self.signal.test_present(runtime, move |runtime: &mut Runtime, status: bool| {
            next.call(runtime, (UCPresent {signal: sig}, status))
        });
    }
}