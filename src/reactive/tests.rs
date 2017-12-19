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
fn question2() {
    let n = Rc::new(RefCell::new(0));
    let nn = n.clone();
    let mut runtime = Runtime::new();
    let cont_print = Box::new(move |_run :&mut Runtime, ()| *nn.borrow_mut() = 42);
    let cont_wait = Box::new(|run :&mut Runtime, ()| run.on_next_instant(cont_print));
    runtime.on_current_instant(cont_wait);
    assert_eq!(*n.borrow(), 0);
    assert!(runtime.instant());
    assert_eq!(*n.borrow(), 0);
    assert!(!runtime.instant());
    assert_eq!(*n.borrow(), 42);
}

#[test]
fn question5() {
    let n = Rc::new(RefCell::new(0));
    let nn = n.clone();
    let mut runtime = Runtime::new();
    let cont_print = Box::new(move |_run :&mut Runtime, ()| *nn.borrow_mut() = 42);
    let cont_wait = Box::new(cont_print.pause());
    runtime.on_current_instant(cont_wait);
    assert_eq!(*n.borrow(), 0);
    assert!(runtime.instant());
    assert_eq!(*n.borrow(), 0);
    assert!(!runtime.instant());
    assert_eq!(*n.borrow(), 42);
}

#[test]
fn test_flatten() {
    let n = Rc::new(RefCell::new(0));
    let nn = n.clone();
    let mut runtime = Runtime::new();
    let p = value(value(42));

    assert_eq!(*n.borrow(), 0);
    p.flatten().call(&mut runtime, move |_: &mut Runtime, val| *nn.borrow_mut() = val);
    assert_eq!(*n.borrow(), 42);
}

#[test]
fn test_pause_process() {
    let n = Rc::new(RefCell::new(0));
    let nn = n.clone();
    let p = value(42).pause().map(move |val| {
        *nn.borrow_mut() = val;
    });

    assert_eq!(*n.borrow(), 0);
    execute_process(p);
    assert_eq!(*n.borrow(), 42);
}

#[test]
fn test_join_process() {
    let n = Rc::new(RefCell::new((0, 0)));
    let nn = n.clone();
    let p = join(value(42), value(1337)).map(move |val| {
        *nn.borrow_mut() = val;
    });

    assert_eq!(*n.borrow(), (0, 0));
    execute_process(p);
    assert_eq!(*n.borrow(), (42, 1337));
}


#[test]
fn test_process_return() {
    assert_eq!(execute_process(value(42)), 42);
}

#[test]
fn test_process_while() {
    let n = Rc::new(RefCell::new(0));
    let nn = n.clone();

    let iter = move |_| {
        let mut x = nn.borrow_mut();
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

    assert_eq!(*n.borrow(), 0);
    execute_process(p);
    assert_eq!(*n.borrow(), 42);
}

#[test]
fn test_process_if() {
    let p = if_else(value(false),
                    if_else(value(true), value(1), value(2)),
                    if_else(value(true), value(3), value(4)));

    assert_eq!(execute_process(p), 3);
}

#[test]
fn test_signal_await() {
    let n = Rc::new(RefCell::new(0));
    let nn = n.clone();
    let nnn = n.clone();
    let nnnn = n.clone();
    let s =  PureSignal::new();

    let p = join(
        s.await_immediate().map(move |()| {
            *nnn.borrow_mut() = 1337;
        }),
        value(()).map(move |()| {
            *nn.borrow_mut() = 42;
        }).pause().then(s.emit()).then(value(()).pause()).map(move |()| {
            *nnnn.borrow_mut() += 1;
        })
    );

    assert_eq!(*n.borrow(), 0);
    execute_process(p);
    assert_eq!(*n.borrow(), 1338);
}

#[test]
fn test_signal_await_2() {
    let n = Rc::new(RefCell::new(0));
    let nn = n.clone();
    let nnn = n.clone();
    let nnnn = n.clone();
    let s = PureSignal::new();
    let sig_ref = s.runtime().clone();
    sig_ref.signal_runtime.lock().unwrap().status = true;

    let p = join(
        s.await_immediate().map(move |()| {
            *nnn.borrow_mut() = 1337;
        }),
        value(()).map(move |()| {
            *nn.borrow_mut() = 42;
        }).pause().then(s.emit()).then(value(()).pause()).map(move |()| {
            *nnnn.borrow_mut() += 1;
        })
    );

    assert_eq!(*n.borrow(), 0);
    execute_process(p);
    assert_eq!(*n.borrow(), 43);
}

#[test]
fn test_signal_present() {
    let s = PureSignal::new();

    let n = Rc::new(RefCell::new(0));
    let m = Rc::new(RefCell::new(0));
    let mm = m.clone();

    let iter = move |_| {
        let mut x = n.borrow_mut();
        *x = *x + 1;
        if *x == 42 {
            return LoopStatus::Exit(());
        } else {
            return LoopStatus::Continue;
        }
    };

    let iter2 = move |_| {
        let mut x = mm.borrow_mut();
        *x = *x + 2;
        LoopStatus::Continue
    };

    let p = s.emit().map(
        iter
    ).pause().while_loop();

    let q = if_else(s.present(), value(()).map(iter2), value(LoopStatus::Exit(()))).pause().while_loop();

    assert_eq!(*m.borrow(), 0);
    execute_process(join(p, q));
    assert_eq!(*m.borrow(), 42 * 2);
}

#[test]
fn test_value_signal() {
    timeout_ms(|| {
        let s: ValueSignal<i32, i32> = ValueSignal::new(0, Box::new(|x, y| x + y));

        assert_eq!(execute_process(join(s.emit(1).then(s.emit(5)), s.await())), ((), 6));
        assert_eq!(execute_process(join(s.emit(1).then(s.emit(5).pause()), s.await())), ((), 1));
        assert_eq!(execute_process(join(
            s.emit(2).then(s.emit(5).pause()).then(s.emit(15).pause()).then(s.emit(15).pause()).then(s.emit(15).pause()).then(s.emit(15).pause()).then(s.emit(15).pause()).then(s.emit(15).pause()),
            join(
                s.await(),
                s.await().then(s.await())
            ).map(|(x, y)| x * y)
        )),
                   ((), 10));
    }, 1000);
}