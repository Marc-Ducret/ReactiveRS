use std::rc::Rc;
use std::cell::RefCell;
use std::cell::Cell;
use std::option::Option;
use std::sync::{Arc, Mutex};
use std;
use std::{thread, time};

mod continuation;
pub mod process;
pub mod pure_signal;
pub mod value_signal;
pub mod unique_consumer_signal;
pub mod unique_producer_signal;
mod tests;

use self::continuation::*;
use self::process::*;
use self::pure_signal::*;
use self::value_signal::*;
use self::unique_consumer_signal::*;
use self::unique_producer_signal::*;

//  ____              _   _
// |  _ \ _   _ _ __ | |_(_)_ __ ___   ___
// | |_) | | | | '_ \| __| | '_ ` _ \ / _ \
// |  _ <| |_| | | | | |_| | | | | | |  __/
// |_| \_\\__,_|_| |_|\__|_|_| |_| |_|\___|


/// Runtime for executing reactive continuations.
pub struct Runtime {
    current_instant: Vec<Box<Continuation<()>>>,
    end_instant: Vec<Box<Continuation<()>>>,
    next_current_instant: Vec<Box<Continuation<()>>>,
    next_end_instant: Vec<Box<Continuation<()>>>,
}

impl Runtime {
    /// Creates a new `Runtime`.
    pub fn new() -> Self {
        Runtime {
            current_instant: Vec::new(),
            end_instant: Vec::new(),
            next_current_instant: Vec::new(),
            next_end_instant: Vec::new(),
        }
    }

    /// Executes instants until all work is completed.
    pub fn execute(&mut self) {
        while self.instant() {}
    }

    /// Executes a single instant to completion. Indicates if more work remains to be done.
    pub fn instant(&mut self) -> bool {
        while let Some(cont) = self.current_instant.pop() {
            cont.call_box(self, ());
        }
        std::mem::swap(&mut self.current_instant, &mut self.next_current_instant);
        std::mem::swap(&mut self.end_instant, &mut self.next_end_instant);
        while let Some(cont) = self.next_end_instant.pop() {
            cont.call_box(self, ());
        }

        (!self.current_instant.is_empty())
     || (!self.end_instant.is_empty())
     || (!self.next_end_instant.is_empty())
    }

    /// Registers a continuation to execute on the current instant.
    fn on_current_instant(&mut self, c: Box<Continuation<()>>) {
        self.current_instant.push(c);
    }

    /// Registers a continuation to execute at the next instant.
    fn on_next_instant(&mut self, c: Box<Continuation<()>>) {
        self.next_current_instant.push(c);
    }

    /// Registers a continuation to execute at the end of the instant. Runtime calls for `c`
    /// behave as if they where executed during the next instant.
    fn on_end_of_instant(&mut self, c: Box<Continuation<()>>) {
        self.end_instant.push(c);
    }
}