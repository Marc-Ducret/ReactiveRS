#![feature(conservative_impl_trait)]
#![type_length_limit="33554432"]

extern crate reactive_rs;

mod redstone;

use reactive_rs::reactive::process::*;
use reactive_rs::reactive::signal::pure_signal::*;
use reactive_rs::reactive::signal::value_signal::*;

use redstone::*;

use std::{thread, time};

fn _main() {
    let s = PureSignal::new();

    let continu: LoopStatus<()> = LoopStatus::Continue;
    let dt = time::Duration::from_millis(100);
    let print_emit = move|_| {
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

fn __main() {
    let s = ValueSignal::new(0, Box::new(|x, y| x+y));

    let conti: LoopStatus<()> = LoopStatus::Continue;
    let mut ps = Vec::new();
    for _ in 0..1000 {
        let sleep = |_| thread::sleep(time::Duration::from_millis(1));
        ps.push(s.emit(value(1)).map(sleep).then(value(conti).pause()).while_loop());
    }
    let p = multi_join(ps);
    let print = |x| {
        println!("x = {}", x);
        x
    };
    let q = s.emit(s.await().map(print)).then(value(conti)).while_loop();
    execute_process(join(p, q));
}

fn main() {
    redstone_sim();
}