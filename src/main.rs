extern crate reactive_rs;

use reactive_rs::reactive::process::*;
use reactive_rs::reactive::pure_signal::*;
use reactive_rs::reactive::value_signal::*;

use std::{thread, time};

fn _main() {
    let s = PureSignal::new();

    let continu: LoopStatus<()> = LoopStatus::Continue;
    let dt = time::Duration::from_millis(100);
    let print_emit = move |_| {
        println!("emit");
        thread::sleep(dt);
    };
    let print_present = |_| println!("present");
    let print_not_present = |_| println!("not present");
    let print_received = |_| println!("received");
    let p = s.emit().map(print_emit).then(value(continu).pause()).while_loop();
    let q = if_else(s.present(),
                    value(()).map(print_present).then(value(()).pause()),
                    value(()).map(print_not_present)
    ).then(value(continu)).while_loop();
    let r = s.await_immediate().map(print_received).then(value(continu).pause()).while_loop();

    execute_process(join(p, join(q, r)));
}

fn main() {
    let s: ValueSignal<i32, i32> = ValueSignal::new(0, Box::new(|x, y| x + y));

//    assert_eq!(execute_process(join(s.emit(1).then(s.emit(5)), s.await())), ((), 6));
//    assert_eq!(execute_process(join(s.emit(1).then(s.emit(5).pause()), s.await())), ((), 1));
    assert_eq!(execute_process(join(
        s.emit(2).then(s.emit(5).pause()).then(s.emit(15).pause()).then(s.emit(15).pause()).then(s.emit(15).pause()).then(s.emit(15).pause()).then(s.emit(15).pause()).then(s.emit(15).pause()),
        join(
            s.await(),
            s.await().then(s.await())
        ).map(|(x, y)| x * y)
    )),
               ((), 10));
}