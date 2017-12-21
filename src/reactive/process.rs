use super::*;

//  ____
// |  _ \ _ __ ___   ___ ___  ___ ___
// | |_) | '__/ _ \ / __/ _ \/ __/ __|
// |  __/| | | (_) | (_|  __/\__ \__ \
// |_|   |_|  \___/ \___\___||___/___/


/// A reactive process.
pub trait Process: Send + Sync + 'static {
    /// The value created by the process.
    type Value: Send + Sync;

    /// Executes the reactive process in the runtime, calls `next` with the resulting value.
    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value>;

    fn map<F, V2>(self, map: F) -> Map<Self, F> where Self: Sized, F: FnOnce(Self::Value) -> V2 + 'static {
        Map { process: self, map }
    }

    fn pause(self) -> Pause<Self> where Self: Sized + 'static {
        Pause { process: self }
    }

    fn flatten(self) -> Flatten<Self> where Self: Sized, Self::Value: Process {
        Flatten { process: self }
    }

    fn and_then<F, P>(self, then: F) -> Flatten<Map<Self, F>> where Self: Sized, F: Fn(Self::Value) -> P + Send + Sync + 'static, P: Process {
        self.map(then).flatten()
    }

    fn then<P>(self, process: P) -> Then<Self, P> where Self: Sized, P: Process {
        Then {p: self, q: process}
    }

    fn join<P>(self, process: P) -> Join<Self, P> where Self: Sized, P: Process {
        Join {
            p1: self,
            p2: process
        }
    }
}

pub struct Then<P, Q> {
    p: P,
    q: Q,
}

impl<P, Q> Process for Then<P, Q> where P: Process, Q: Process {
    type Value = Q::Value;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        let p = self.p;
        let q = self.q;
        p.call(runtime, move |runtime: &mut Runtime, _| q.call(runtime, next))
    }
}

impl<P, Q> ProcessMut for Then<P, Q> where P: ProcessMut, Q: ProcessMut {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, Self::Value)> {
        let p = self.p;
        let q = self.q;
        p.call_mut(runtime, move |runtime: &mut Runtime, (p, _): (P, P::Value)|
            q.call_mut(runtime, |runtime: &mut Runtime, (q, value): (Q, Q::Value)|
                next.call(runtime, (p.then(q), value))
            )
        )
    }
}

/// A process that can be executed multiple times, modifying its environment each time.
pub trait ProcessMut: Process {
    /// Executes the mutable process in the runtime, then calls `next` with the process and the
    /// process's return value.
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where
        Self: Sized, C: Continuation<(Self, Self::Value)>;

    fn while_loop<V>(self) -> While<Self> where Self: ProcessMut<Value = LoopStatus<V>>, Self: Sized, V: Send + Sync {
        While {process: self}
    }
}

/// Indicates if a loop is finished.
#[derive(Copy, Clone)]
pub enum LoopStatus<V> { Continue, Exit(V) }

pub fn execute_process<P>(p: P) -> P::Value where P: Process {
    let mut runtime = SequentialRuntime::new();
    let result = Arc::new(Mutex::new(None));
    let result_ref = result.clone();
    runtime.on_current_instant(Box::new(|run: &mut Runtime, _|
        p.call(run, move |_: &mut Runtime, val| {
            let mut res = result_ref.lock().unwrap();
            *res = Some(val);
        })
    ));
    runtime.execute();
    let mut res = None;
    std::mem::swap(&mut res, &mut *result.lock().unwrap());
    if let Some(res) = res {
        return res;
    } else {
        panic!("No result from execute?! (result continuation was probably lost)");
    }
}

pub struct Value<T> {
    val: T
}

impl<T: 'static> Process for Value<T> where T: Send + Sync {
    type Value = T;
    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        next.call(runtime, self.val)
    }
}

impl<T: 'static> ProcessMut for Value<T> where T: Copy + Send + Sync {
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
            p.call_mut(runtime, next.map(|(_, v)| (process.flatten(), v)))
        );
    }
}

pub struct Map<P, F> { process: P, map: F }

impl<F, V, P> Process for Map<P, F>
    where P: Process, F: FnOnce(P::Value) -> V + Send + Sync + 'static, V: Send + Sync  {
    type Value = V;
    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        //self.continuation is a process
        let f = self.map;
        (self.process).call(runtime, move |runtime: &mut Runtime, x| (next.call(runtime, f(x))))
    }
}

impl<F, V, P> ProcessMut for Map<P, F>
    where P: ProcessMut, F: FnMut(P::Value) -> V + Send + Sync + 'static, V: Send + Sync  {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, Self::Value)> {
        //self.continuation is a process
        let mut f: F = self.map;
        self.process.call_mut(runtime, move |runtime: &mut Runtime, (p, x): (P, P::Value)| {
            let y = f(x);
            next.call(runtime, (p.map(f), y))
        })
    }
}

pub struct Pause<P> { process: P }

impl<P> Process for Pause<P> where P: Process {
    type Value = P::Value;
    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        let process = self.process;
        runtime.on_next_instant(Box::new(|run: &mut Runtime, _| process.call(run, next)))
    }
}

impl<P> ProcessMut for Pause<P> where P: ProcessMut {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, Self::Value)> {
        let process = self.process;
        runtime.on_next_instant(Box::new(|run: &mut Runtime, _|
            process.call_mut(run, next.map(
                |(p, x): (P, P::Value)| (p.pause(), x)
            ))
        ))
    }
}

pub struct Join<P1, P2> { p1: P1, p2: P2 }

impl<P1, P2> Process for Join<P1, P2> where P1: Process, P2: Process {
    type Value = (P1::Value, P2::Value);
    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        struct JoinPoint<T1, T2, C> where T1: Send + Sync, T2: Send + Sync {
            x1: Option<T1>,
            x2: Option<T2>,
            next: Option<C>
        }

        impl<T1, T2, C> JoinPoint<T1, T2, C> where C: Continuation<(T1, T2)>, T1: Send + Sync, T2: Send + Sync {
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

        let jp = Arc::new(Mutex::new(JoinPoint{x1: None, x2: None, next: Some(next)}));

        {
            let jp = jp.clone();
            self.p1.call(runtime, move |run: &mut Runtime, x1| {
                let mut jp = jp.lock().unwrap();
                jp.x1 = Some(x1);
                jp.call_continuation(run)
            });
        }

        {
            let jp = jp.clone();
            self.p2.call(runtime, move |run: &mut Runtime, x2| {
                let mut jp = jp.lock().unwrap();
                jp.x2 = Some(x2);
                jp.call_continuation(run)
            });
        }
    }
}

impl<P1, P2> ProcessMut for Join<P1, P2> where P1: ProcessMut, P2: ProcessMut {
    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, Self::Value)> {
        struct JoinPoint<T1, T2, P1, P2, C> where P1: ProcessMut, P2: ProcessMut {
            x1: Option<T1>,
            x2: Option<T2>,
            op1: Option<P1>,
            op2: Option<P2>,
            next: Option<C>
        }

        impl<T1, T2, P1, P2, C> JoinPoint<T1, T2, P1, P2, C> where C: Continuation<(Join<P1, P2>, (T1, T2))>, P1: ProcessMut, P2: ProcessMut, T1: Send + Sync, T2: Send + Sync {
            fn call_continuation(&mut self, run: &mut Runtime, mut op1: Option<P1>, mut op2: Option<P2>) {
                if op1.is_some() {
                    self.op1 = op1.take();
                }
                if op2.is_some() {
                    self.op2 = op2.take();
                }
                if self.x1.is_some() {
                    if self.x2.is_some() {
                        let next = self.next.take();
                        let x1 = self.x1.take();
                        let x2 = self.x2.take();
                        let op1 = self.op1.take();
                        let op2 = self.op2.take();
                        if let Some(y1) = x1 {
                            if let Some(y2) = x2 {
                                if let Some(cont) = next {
                                    if let Some(p1) = op1 {
                                        if let Some(p2) = op2 {
                                            cont.call(run, (Join {p1, p2}, (y1, y2)));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };

        let jp = Arc::new(Mutex::new(JoinPoint{x1: None, x2: None, op1: None, op2: None, next: Some(next)}));


        {
            let jp = jp.clone();
            self.p1.call_mut(runtime, move |run: &mut Runtime, (p1, x1)| {
                let mut jp = jp.lock().unwrap();
                jp.x1 = Some(x1);
                jp.call_continuation(run, Some(p1), None)
            });
        }

        {
            let jp = jp.clone();
            self.p2.call_mut(runtime, move |run: &mut Runtime, (p2, x2)| {
                let mut jp = jp.lock().unwrap();
                jp.x2 = Some(x2);
                jp.call_continuation(run, None, Some(p2))
            });
        }
    }
}

pub fn join<P1, P2>(p1: P1, p2: P2) -> Join<P1, P2> {
    Join {p1, p2}
}

pub struct While<P> {
    process: P
}

impl<P, V> Process for While<P> where P: ProcessMut<Value = LoopStatus<V>>, V: Send + Sync + 'static {
    type Value = V;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<Self::Value> {
        self.process.call_mut(runtime, |runtime: &mut Runtime, (p, loop_status): (P, LoopStatus<V>)|
            match loop_status {
                LoopStatus::Continue => p.while_loop().call(runtime, next),
                LoopStatus::Exit(value) => return next.call(runtime, value)
            }
        );
    }
}

pub fn if_else<P, Q, R>(r: R, p: P, q: Q) -> If<P, Q, R> {
    If {process_if: p, process_else: q, process_cond: r}
}

pub struct If<P, Q, R> {
    process_if: P,
    process_else: Q,
    process_cond: R,
}

impl<P, Q, R, V> Process for If<P, Q, R> where P: Process<Value = V>, Q: Process<Value = V>, R: Process<Value = bool>, V: Send + Sync {
    type Value = V;

    fn call<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<V> {
        let p = self.process_if;
        let q = self.process_else;
        let r = self.process_cond;
        r.call(runtime, move |runtime: &mut Runtime, cond: bool| {
            if cond {
                p.call(runtime, next);
            } else {
                q.call(runtime, next);
            }
        });
    }
}

impl<P, Q, R, V> ProcessMut for If<P, Q, R> where P: ProcessMut<Value = V>, Q: ProcessMut<Value = V>, R: ProcessMut<Value = bool>, V: Send + Sync {

    fn call_mut<C>(self, runtime: &mut Runtime, next: C) where C: Continuation<(Self, V)> {
        let p = self.process_if;
        let q = self.process_else;
        let r = self.process_cond;
        r.call_mut(runtime, move |runtime: &mut Runtime, (r, cond): (R, bool)| {
            if cond {
                p.call_mut(runtime, next.map(|(p, v): (P, V)| (if_else(r, p, q), v)));
            } else {
                q.call_mut(runtime, next.map(|(q, v): (Q, V)| (if_else(r, p, q), v)));
            }
        });
    }
}
