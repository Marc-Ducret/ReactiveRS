extern crate reactive_rs;

use reactive_rs::reactive::process::*;
use reactive_rs::reactive::signal::value_signal::*;

use std::cmp::max;

pub fn redstone_sim() {
    let h = 2;
    let w = 2;

    let mut sigs = Vec::new();
    for _ in 0..(w*h) {
        sigs.push(ValueSignal::new(0, Box::new(|x: i32, y: i32| max(x, y))));
    }
    let sig_at = |x: usize, y: usize| sigs[(x % w) + (y % h) * w].clone();
    let mut processes = Vec::new();
    for x in 0..w {
        for y in 0..h {
            processes.push(redstone_block_process(sig_at(x  , y),
                                                  sig_at(x + 1, y    ),
                                                  sig_at(x - 1, y    ),
                                                  sig_at(x    , y + 1),
                                                  sig_at(x    , y - 1)));
        }
    }
    execute_process_par(multi_join(processes).join(sigs[0].clone().emit(value(15))).join(display_process(&sigs)));
}

fn redstone_block_process(input: ValueSignal<i32, i32>, out_l: ValueSignal<i32, i32>, out_r: ValueSignal<i32, i32>, out_u: ValueSignal<i32, i32>, out_d: ValueSignal<i32, i32>) -> impl Process {
    let decr = |x: i32| {
        use std::thread;
        thread::sleep_ms(100);
        x-1
    };
    let continue_loop: LoopStatus<()> = LoopStatus::Continue;
    input.emit(out_l.emit(out_r.emit(out_u.emit(out_d.emit(input.await().map(decr)))))).then(value(continue_loop)).while_loop()
}

fn display_process(sigs: &Vec<ValueSignal<i32, i32>>) -> impl Process {
    let mut processes = Vec::new();
    for s in sigs {
        processes.push(s.await());
    }
    let display = |vals: Vec<i32>| {
        for v in vals {
          print!("{}", v);
        }
        println!();
    };
    let continue_loop: LoopStatus<()> = LoopStatus::Continue;
    multi_join(processes).map(display).then(value(continue_loop)).while_loop()
}