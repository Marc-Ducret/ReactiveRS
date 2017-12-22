extern crate reactive_rs;

use reactive_rs::reactive::process::*;
use reactive_rs::reactive::signal::value_signal::*;

use std::cmp::max;
use std::sync::{Arc, Mutex};

#[derive(PartialEq, Clone, Copy)]
enum Direction {
    SOUTH,
    NORTH,
    EAST,
    WEST
}

enum Type {
    VOID,
    BLOCK,
    REDSTONE(bool, bool, bool),
    INVERTER(Direction),
}

fn displace((x, y): (usize, usize), dir: Direction) -> (usize, usize){
    match dir {
        Direction::SOUTH => return (x  , y-1),
        Direction::NORTH => return (x  , y+1),
        Direction::EAST  => return (x+1, y  ),
        Direction::WEST  => return (x-1, y  ),
    }
}

fn invert_dir(dir: Direction) -> Direction {
    match dir {
        Direction::SOUTH => return Direction::NORTH,
        Direction::NORTH => return Direction::SOUTH,
        Direction::EAST  => return Direction::WEST,
        Direction::WEST  => return Direction::EAST,
    }
}

pub fn redstone_sim() {
    let h = 32;
    let w = 32;

    let mut blocks = Vec::new();
    for _ in 0..(w*h) {
        blocks.push(Type::VOID);
    }
    for x in 0..w {
        blocks[x] = Type::REDSTONE(true, true, true);
    }
    blocks[5] = Type::INVERTER(Direction::EAST);

    let mut power_signal = Vec::new();
    for _ in 0..(w*h) {
        power_signal.push(ValueSignal::new(0, Box::new(|x: i32, y: i32| max(x, y))));
    }
    let display_signal = ValueSignal::new(vec!(), Box::new(|entries: Vec<(usize, usize, i32)>, entry: (usize, usize, i32)| {
        let mut entries = entries.clone();
        entries.push(entry);
        entries
    }));
    let power_at = |(x, y): (usize, usize)| power_signal[(x % w) + (y % h) * w].clone();

    let redstone_wire_process = |x: usize, y: usize| {
        let decr = |x: i32| {
            use std::thread;
            thread::sleep_ms(100);
            x-1
        };
        let continue_loop: LoopStatus<()> = LoopStatus::Continue;
        let input = power_at((x, y));
        let combine_with_pos = move|power| (x, y, power);
        display_signal.emit(input.emit(
            power_at((x + 1, y    )).emit(
                power_at((x - 1, y    )).emit(
                    power_at((x    , y + 1)).emit(
                        power_at((x    , y - 1)).emit(input.await().map(decr))))))
                                .map(combine_with_pos)).then(value(continue_loop)).while_loop()
    };

    let redstone_torch_process = |x: usize, y: usize, dir: Direction| {
        let input = power_at(displace((x, y), invert_dir(dir)));
        let is_powered = |power| {
            power > 0
        };
        let mut emit_near = Vec::new();
        for d in vec!(Direction::NORTH, Direction::SOUTH, Direction::EAST, Direction::WEST) {
            if d != invert_dir(dir) {
                emit_near.push(power_at(displace((x, y), d)).emit(value(0xF)))
            }
        }
        let continue_loop: LoopStatus<()> = LoopStatus::Continue;
        input.emit(value(0)).then(if_else(input.await().map(is_powered), value(()), multi_join(emit_near).then(value(())))).then(value(continue_loop)).while_loop()
    };

    let display_process = || {
        let mut powers = Vec::new();
        for _ in 0..(w*h) {
            powers.push(0);
        }
        let powers: Arc<Mutex<Vec<i32>>> = Arc::new(Mutex::new(powers));
        let continue_loop: LoopStatus<()> = LoopStatus::Continue;
        let powers_ref = powers.clone();
        let read_entries = move|entries: Vec<(usize, usize, i32)>| {
            let mut powers = powers_ref.lock().unwrap();
            for i in 0..(w*h) {
                (*powers)[i] = 0;
            }
            for (x, y, power) in entries {
                (*powers)[x + y * w] = power+1;
            }
        };
        let powers_ref = powers.clone();
        let draw = move|_| {
            let powers = powers_ref.lock().unwrap();
            for y in 0..h {
                for x in 0..w {
                    print!("{}", (*powers)[x + y * w]);
                }
                println!();
            }
            println!("----------------------------")
        };
        display_signal.await().map(read_entries).map(draw).then(value(continue_loop)).while_loop()
    };

    let mut p_redstone = Vec::new();
    let mut p_inverter = Vec::new();
    for x in 0..w {
        for y in 0..h {
            match blocks[x + y * w] {
                Type::VOID => (),
                Type::BLOCK => (),
                Type::REDSTONE(r, g, b) => p_redstone.push(redstone_wire_process(x, y)),
                Type::INVERTER(dir) => p_inverter.push(redstone_torch_process(x, y, dir)),
            }
        }
    }
    execute_process_par(multi_join(p_redstone).join(multi_join(p_inverter)).join(display_process()));
}