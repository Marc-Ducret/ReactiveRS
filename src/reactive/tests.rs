extern crate timebomb;

use std::thread;
use self::timebomb::{timeout_ms};

use super::*;

//  _____         _
// |_   _|__  ___| |_ ___
//   | |/ _ \/ __| __/ __|
//   | |  __/\__ \ |_\__ \
//   |_|\___||___/\__|___/


#[test]
fn test_continuation() {
    let n = Arc::new(Mutex::new(0));
    let nn = n.clone();
    let mut runtime = SequentialRuntime::new();
    let cont_print = Box::new(move|_run :&mut Runtime, ()| *nn.lock().unwrap() = 42);
    let cont_wait = Box::new(|run :&mut Runtime, ()| run.on_next_instant(cont_print));
    runtime.on_current_instant(cont_wait);
    assert_eq!(*n.lock().unwrap(), 0);
    assert!(runtime.instant());
    assert_eq!(*n.lock().unwrap(), 0);
    assert!(!runtime.instant());
    assert_eq!(*n.lock().unwrap(), 42);
}

#[test]
fn test_continuation_pause() {
    let n = Arc::new(Mutex::new(0));
    let nn = n.clone();
    let mut runtime = SequentialRuntime::new();
    let cont_print = Box::new(move|_run :&mut Runtime, ()| *nn.lock().unwrap() = 42);
    let cont_wait = Box::new(cont_print.pause());
    runtime.on_current_instant(cont_wait);
    assert_eq!(*n.lock().unwrap(), 0);
    assert!(runtime.instant());
    assert_eq!(*n.lock().unwrap(), 0);
    assert!(!runtime.instant());
    assert_eq!(*n.lock().unwrap(), 42);
}

#[test]
fn test_process_flatten() {
    let n = Arc::new(Mutex::new(0));
    let nn = n.clone();
    let mut runtime = SequentialRuntime::new();
    let p = value(value(42));

    assert_eq!(*n.lock().unwrap(), 0);
    p.flatten().call(&mut runtime, move|_: &mut Runtime, val| *nn.lock().unwrap() = val);
    assert_eq!(*n.lock().unwrap(), 42);
}

#[test]
fn test_process_pause() {
    let n = Arc::new(Mutex::new(0));
    let nn = n.clone();
    let p = value(42).pause().map(move|val| {
        *nn.lock().unwrap() = val;
    });

    assert_eq!(*n.lock().unwrap(), 0);
    execute_process_par(p);
    assert_eq!(*n.lock().unwrap(), 42);
}

#[test]
fn test_process_join() {
    let n = Arc::new(Mutex::new((0, 0)));
    let nn = n.clone();
    let p = join(value(42), value(1337)).map(move|val| {
        *nn.lock().unwrap() = val;
    });

    assert_eq!(*n.lock().unwrap(), (0, 0));
    execute_process_par(p);
    assert_eq!(*n.lock().unwrap(), (42, 1337));
}


#[test]
fn test_process_return() {
    assert_eq!(execute_process_par(value(42)), 42);
}

#[test]
fn test_process_while() {
    let n = Arc::new(Mutex::new(0));
    let nn = n.clone();

    let iter = move|_| {
        let mut x = nn.lock().unwrap();
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

    assert_eq!(*n.lock().unwrap(), 0);
    execute_process_par(p);
    assert_eq!(*n.lock().unwrap(), 42);
}

#[test]
fn test_process_if() {
    let p = if_else(value(false),
                    if_else(value(true), value(1), value(2)),
                    if_else(value(true), value(3), value(4)));

    assert_eq!(execute_process_par(p), 3);
}

#[test]
fn test_signal_await() {
    let n = Arc::new(Mutex::new(0));
    let nn = n.clone();
    let nnn = n.clone();
    let nnnn = n.clone();
    let s =  PureSignal::new();

    let p = join(
        s.await_immediate().map(move|()| {
            *nnn.lock().unwrap() = 1337;
        }),
        value(()).map(move|()| {
            *nn.lock().unwrap() = 42;
        }).pause().then(s.emit()).then(value(()).pause()).map(move|()| {
            *nnnn.lock().unwrap() += 1;
        })
    );

    assert_eq!(*n.lock().unwrap(), 0);
    execute_process_par(p);
    assert_eq!(*n.lock().unwrap(), 1338);
}

#[test]
fn test_signal_await_2() {
    let n = Arc::new(Mutex::new(0));
    let nn = n.clone();
    let nnn = n.clone();
    let nnnn = n.clone();
    let s = PureSignal::new();
    let sig_ref = s.runtime().clone();
    sig_ref.signal_runtime.lock().unwrap().status = true;

    let p = join(
        s.await_immediate().map(move|()| {
            *nnn.lock().unwrap() = 1337;
        }),
        value(()).map(move|()| {
            *nn.lock().unwrap() = 42;
        }).pause().then(s.emit()).then(value(()).pause()).map(move|()| {
            *nnnn.lock().unwrap() += 1;
        })
    );

    assert_eq!(*n.lock().unwrap(), 0);
    execute_process_par(p);
    assert_eq!(*n.lock().unwrap(), 43);
}

#[test]
fn test_signal_present() {
    let s = PureSignal::new();

    let n = Arc::new(Mutex::new(0));
    let m = Arc::new(Mutex::new(0));
    let mm = m.clone();

    let iter = move|_| {
        let mut x = n.lock().unwrap();
        *x = *x + 1;
        if *x == 42 {
            return LoopStatus::Exit(());
        } else {
            return LoopStatus::Continue;
        }
    };

    let iter2 = move|_| {
        let mut x = mm.lock().unwrap();
        *x = *x + 2;
        LoopStatus::Continue
    };

    let p = s.emit().map(
        iter
    ).pause().while_loop();

    let q = if_else(s.present(), value(()).map(iter2), value(LoopStatus::Exit(()))).pause().while_loop();

    assert_eq!(*m.lock().unwrap(), 0);
    execute_process_par(join(p, q));
    assert_eq!(*m.lock().unwrap(), 42 * 2);
}

#[test]
fn test_value_signal() {
    timeout_ms(|| {
        let s: ValueSignal<i32, i32> = ValueSignal::new(0, Box::new(|x, y| x + y));

        assert_eq!(execute_process_par(join(s.emit(value(1)).then(s.emit(value(5))), s.await())), ((), 6));
        assert_eq!(execute_process_par(join(s.emit(value(1)).then(s.emit(value(5)).pause()), s.await())), ((), 1));
        for _ in 0..100 {
            assert_eq!(execute_process_par(join(
                s.emit(value(2)).then(s.emit(value(5)).pause()).then(s.emit(value(15)).pause()),
                join(
                    s.await(),
                    s.await().then(s.await())
                ).map(|(x, y)| {
                    x * y
                })
            )),
                       ((), 10));
        }
    }, 5000);
}

#[test]
fn test_unique_consumer_signal() {
    let (s_prod, s_cons): (UniqueConsumerSignalProducer<Vec<i32>, i32>, UniqueConsumerSignalConsumer<Vec<i32>, i32>) =
        UniqueConsumerSignalProducer::new(
            Box::new(|| vec![]),
            Box::new(|mut v, x| {
                v.push(x);
                v
            }));

    assert_eq!(execute_process_par(join(s_prod.emit(value(1)).then(s_prod.emit(value(5))), s_cons.await())), ((), vec![1, 5]));

    let (s_prod, s_cons): (UniqueConsumerSignalProducer<Vec<i32>, i32>, UniqueConsumerSignalConsumer<Vec<i32>, i32>) =
        UniqueConsumerSignalProducer::new(
            Box::new(|| vec![]),
            Box::new(|mut v, x| {
              v.push(x);
              v
            }));

    assert_eq!(execute_process_par(join(s_prod.emit(value(1)).then(s_prod.emit(value(5)).pause()), s_cons.await())), ((), vec![1]));
}

#[test]
fn test_unique_producer_signal() {
    let (s_prod, s_cons): (UniqueProducerSignalProducer<i32>, UniqueProducerSignalConsumer<i32>) =
        UniqueProducerSignalProducer::new(0);

    assert_eq!(execute_process_par(join(s_prod.emit(value(1)), join(s_cons.await_immediate(), s_cons.await_immediate()))), ((), (1, 1)));
}

#[test]
fn test_parallel() {
    assert_eq!(execute_process_par(join(value(15), value(1337))), (15, 1337));
}