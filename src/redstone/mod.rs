extern crate reactive_rs;

use reactive_rs::reactive::process::*;
use reactive_rs::reactive::signal::value_signal::*;

use std::ops::{Add, Sub};
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

#[derive(PartialEq, Clone, Copy)]
struct Power {
    r: u8,
    g: u8,
    b: u8,
}

impl Add for Power {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        return Power{
            r: self.r + other.r,
            g: self.g + other.g,
            b: self.b + other.b}
    }
}

impl Sub for Power {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        return Power{
            r: self.r - other.r,
            g: self.g - other.g,
            b: self.b - other.b}
    }
}

fn max_p(p: Power, q: Power) -> Power {
    Power{
        r: max(p.r, q.r),
        g: max(p.r, q.r),
        b: max(p.r, q.r)}
}

const ZERO_POWER: Power = Power{r: 0x0, g: 0x0, b: 0x0};
const ATOMIC_POWER: Power = Power{r: 0x1, g: 0x1, b: 0x1};
const MAX_POWER: Power = Power{r: 0xF, g: 0xF, b: 0xF};

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
    blocks[19] = Type::INVERTER(Direction::EAST);
    blocks[30] = Type::INVERTER(Direction::EAST);

    let mut power_signal = Vec::new();
    for _ in 0..(w*h) {
        power_signal.push(ValueSignal::new(ZERO_POWER, Box::new(|x: Power, y: Power| max_p(x, y))));
    }
    let display_signal = ValueSignal::new(vec!(), Box::new(|entries: Vec<(usize, usize, Power)>, entry: (usize, usize, Power)| {
        let mut entries = entries.clone();
        entries.push(entry);
        entries
    }));
    let power_at = |(x, y): (usize, usize)| power_signal[(x % w) + (y % h) * w].clone();

    let redstone_wire_process = |x: usize, y: usize| {
        let decr = |x: Power| {
            use std::thread;
            thread::sleep_ms(100);
            max_p(x, ATOMIC_POWER) - ATOMIC_POWER
        };
        let continue_loop: LoopStatus<()> = LoopStatus::Continue;
        let input = power_at((x, y));
        let combine_with_pos = move|power| (x, y, power);
        let uncombine = move|(x, y, power)| power;
        input.emit(
            power_at((x + 1, y    )).emit(
                power_at((x - 1, y    )).emit(
                    power_at((x    , y + 1)).emit(
                        power_at((x    , y - 1)).emit(
                            display_signal.emit(
                                input.await().map(combine_with_pos)).map(uncombine).map(decr))))))
            .then(value(continue_loop)).while_loop()
    };

    let redstone_torch_process = |x: usize, y: usize, dir: Direction| {
        let input = power_at(displace((x, y), invert_dir(dir)));
        let is_powered = |power| {
            power != ZERO_POWER
        };
        let mut emit_near = Vec::new();
        for d in vec!(Direction::NORTH, Direction::SOUTH, Direction::EAST, Direction::WEST) {
            if d != invert_dir(dir) {
                emit_near.push(power_at(displace((x, y), d)).emit(value(MAX_POWER)))
            }
        }
        let continue_loop: LoopStatus<()> = LoopStatus::Continue;
        input.emit(value(ZERO_POWER)).then(if_else(input.await().map(is_powered), value(()), multi_join(emit_near).then(value(())))).then(value(continue_loop)).while_loop()
    };

    let display_process = || {
        let mut powers = Vec::new();
        for _ in 0..(w*h) {
            powers.push(ZERO_POWER);
        }
        let powers: Arc<Mutex<Vec<Power>>> = Arc::new(Mutex::new(powers));
        let continue_loop: LoopStatus<()> = LoopStatus::Continue;
        let powers_ref = powers.clone();
        let read_entries = move|entries: Vec<(usize, usize, Power)>| {
            let mut powers = powers_ref.lock().unwrap();
            for i in 0..(w*h) {
                (*powers)[i] = ZERO_POWER;
            }
            for (x, y, power) in entries {
                (*powers)[x + y * w] = power;
            }
        };
        let powers_ref = powers.clone();
        let draw = move|_| {
            let powers = powers_ref.lock().unwrap();
            for y in 0..h {
                for x in 0..w {
                    print!("{:X}", (*powers)[x + y * w].r);
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