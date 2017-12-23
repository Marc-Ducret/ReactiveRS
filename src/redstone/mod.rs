extern crate std;
extern crate reactive_rs;
extern crate piston;
extern crate graphics;
extern crate glutin_window;
extern crate opengl_graphics;

use self::piston::window::WindowSettings;
use self::piston::event_loop::*;
use self::piston::input::*;
use self::glutin_window::GlutinWindow as Window;
use self::opengl_graphics::{ GlGraphics, OpenGL };

use reactive_rs::reactive::process::*;
use reactive_rs::reactive::signal::value_signal::*;

use std::ops::{Add, Sub};
use std::cmp::max;
use std::sync::{Arc, Mutex};
use std::thread;
use std::fs::File;
use std::io::prelude::*;



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

fn read_file(filename: String) -> (Vec<Type>, usize, usize) {
    let mut file = File::open(filename).unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();

    let mut blocks: Vec<Type> = Vec::new();
    let mut width = 0;
    let mut height = 0;

    let mut lines = contents.lines();
    while let Some(line) = lines.next() {
        if height == 0 {
            width = line.len();
        } else {
            assert_eq!(width, line.len());
        }
        height += 1;
        let mut chars = line.chars();
        while let Some(ch) = chars.next() {
            blocks.push(match ch {
                ' ' => Type::VOID,
                '#' => Type::BLOCK,
                'r' => Type::REDSTONE(true,  false, false),
                'g' => Type::REDSTONE(false, true,  false),
                'b' => Type::REDSTONE(false, false, true ),
                'y' => Type::REDSTONE(true,  true,  false),
                'p' => Type::REDSTONE(true,  false, true ),
                'c' => Type::REDSTONE(false, true,  true ),
                'w' => Type::REDSTONE(true,  true,  true ),
                '^' => Type::INVERTER(Direction::NORTH),
                'v' => Type::INVERTER(Direction::SOUTH),
                '<' => Type::INVERTER(Direction::WEST),
                '>' => Type::INVERTER(Direction::EAST),
                _ => panic!("Not a valid character")
            });
        }
    }

    (blocks, width, height)
}

pub struct App {
    gl: GlGraphics, // OpenGL drawing backend.
    powers: Vec<Power>,
    blocks: Vec<Type>,
    width: usize,
    height: usize
}

impl App {
    fn render(&mut self, args: &RenderArgs) {
        use self::graphics::*;

        const VOID_COLOR:       [f32; 4] = [0.0, 0.0, 0.0, 1.0];
        const BLOCK_COLOR_OUT:  [f32; 4] = [0.9, 0.9, 0.9, 1.0];
        const BLOCK_COLOR_IN:   [f32; 4] = [0.5, 0.5, 0.5, 1.0];
        const RED:   [f32; 4] = [1.0, 0.0, 0.0, 1.0];
        const PIXEL_SIZE:  f64 = 10.0;
        const BORDER_SIZE: f64 = 2.0;
        const POWER_MAX:   u8  = 15;

        let square = rectangle::square(0.0, 0.0, PIXEL_SIZE);
        let inner_square = rectangle::square(0.0, 0.0, PIXEL_SIZE-2.0*BORDER_SIZE);
        let rect = rectangle::rectangle_by_corners(0.0, 0.0, PIXEL_SIZE, PIXEL_SIZE/3.0);

        for i in 0..(self.width*self.height) {
            let (ix, iy) = (i%self.width, i/self.width);
            let (x, y) = ((ix as f64)*PIXEL_SIZE, (iy as f64)*PIXEL_SIZE);

            match self.blocks[i] {
                Type::VOID => {
                    self.gl.draw(args.viewport(), |c, gl| {
                        let transform = c.transform.trans(x, y);
                        rectangle(VOID_COLOR, square, transform, gl);
                    });
                },
                Type::BLOCK => {
                    self.gl.draw(args.viewport(), |c, gl| {
                        let transform = c.transform.trans(x, y);
                        rectangle(BLOCK_COLOR_OUT, square, transform, gl);
                        let transform = c.transform.trans(x+BORDER_SIZE, y+BORDER_SIZE);
                        rectangle(BLOCK_COLOR_IN, inner_square, transform, gl);
                    });
                },
                Type::REDSTONE(r, g, b) => {
                    fn color_composant(is_present: bool, power: u8) -> f32 {
                        if is_present { 0.5 + 0.5*((power as f32)/(POWER_MAX as f32)) } else { 0.0 }
                    }
                    let color: [f32; 4] = [
                        color_composant(r, self.powers[i].r),
                        color_composant(g, self.powers[i].g),
                        color_composant(b, self.powers[i].b),
                        1.0
                    ];
                    self.gl.draw(args.viewport(), |c, gl| {
                        let transform = c.transform.trans(x, y);
                        rectangle(color, square, transform, gl);
                    });
                },
                Type::INVERTER(ref dir) => {
                    self.gl.draw(args.viewport(), |c, gl| {
                        let pi = std::f64::consts::PI;
                        let angle = pi/2.0 * match *dir {
                            Direction::SOUTH => 0.0,
                            Direction::NORTH => 2.0,
                            Direction::EAST => 3.0,
                            Direction::WEST => 1.0
                        };
                        let transform = c.transform.trans(x, y).trans(PIXEL_SIZE/2.0, PIXEL_SIZE/2.0).rot_rad(angle).trans(-PIXEL_SIZE/2.0, -PIXEL_SIZE/2.0);
                        let transform2 = transform.rot_rad(pi/2.0).trans(0.0, -PIXEL_SIZE*(0.5+1.0/6.0));
                        rectangle(VOID_COLOR, square, transform, gl);
                        rectangle(RED, rect, transform, gl);
                        rectangle(RED, rect, transform2, gl);
                    });
                }
            }
        }
    }

    fn update(&mut self, args: &UpdateArgs) {
        // args.dt
    }
}

pub fn redstone_sim() {
    let (blocks, w, h) = read_file(String::from("map.txt"));

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
        let uncombine = move|(_x, _y, power)| power;
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

    let display_powers: Arc<Mutex<Vec<Power>>> = Arc::new(Mutex::new(vec![ZERO_POWER; w*h]));
    let display_powers_ref = display_powers.clone();

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
            let mut dpowers = display_powers_ref.lock().unwrap();
            let powers = powers_ref.lock().unwrap();
            dpowers.clone_from(&powers);
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
                Type::REDSTONE(_, _, _) => p_redstone.push(redstone_wire_process(x, y)),
                Type::INVERTER(dir) => p_inverter.push(redstone_torch_process(x, y, dir)),
            }
        }
    }

    let display_powers_ref = display_powers.clone();
    thread::spawn(move || {
        //let opengl = OpenGL::V2_1;
        let opengl = OpenGL::V3_2;

        let mut window: Window = WindowSettings::new(
                "redstone",
                [500, 500]
            )
            .opengl(opengl)
            .exit_on_esc(true)
            .srgb(false) // Necessary due to issue #139 of piston_window.
            .build()
            .unwrap();

        let mut app = App {
            gl: GlGraphics::new(opengl),
            powers: vec![ZERO_POWER; blocks.len()],
            blocks: blocks,
            width: w,
            height: h
        };

        let mut events = Events::new(EventSettings::new());
        while let Some(e) = events.next(&mut window) {
            if let Some(r) = e.render_args() {
                {
                    let mut dpowers = display_powers_ref.lock().unwrap();
                    app.powers.clone_from(&dpowers)
                }
                app.render(&r);
            }

            if let Some(u) = e.update_args() {
                app.update(&u);
            }
        }
    });

    execute_process_par(multi_join(p_redstone).join(multi_join(p_inverter)).join(display_process()));

}
