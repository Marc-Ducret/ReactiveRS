extern crate crossbeam;

use super::*;
use self::crossbeam::sync::MsQueue;

//  ____            ____              _   _
// |  _ \ __ _ _ __|  _ \ _   _ _ __ | |_(_)_ __ ___   ___
// | |_) / _` | '__| |_) | | | | '_ \| __| | '_ ` _ \ / _ \
// |  __/ (_| | |  |  _ <| |_| | | | | |_| | | | | | |  __/
// |_|   \__,_|_|  |_| \_\\__,_|_| |_|\__|_|_| |_| |_|\___|

pub struct ParallelRuntime {
    current_instant: MsQueue<Box<Continuation<()>>>,
    end_instant: MsQueue<Box<Continuation<()>>>,
    next_current_instant: MsQueue<Box<Continuation<()>>>,
    next_end_instant: MsQueue<Box<Continuation<()>>>,
}

impl ParallelRuntime {
    pub fn new() -> Self {
        ParallelRuntime {
            current_instant: MsQueue::new(),
            end_instant: MsQueue::new(),
            next_current_instant: MsQueue::new(),
            next_end_instant: MsQueue::new(),
        }
    }
}

impl ParallelRuntime {
    pub fn execute(&mut self) {
        let workers_count = 12;
        let workers = Vec::with_capacity(workers_count);
        for _ in 0..workers_count {
            workers.push(thread::spawn(move|| {

            }));
        }
        while self.instant() {}
    }

    fn instant(&mut self) -> bool {
        while !self.current_instant.is_empty() {
            self.current_instant.pop().call_box(self, ());
        }
        std::mem::swap(&mut self.current_instant, &mut self.next_current_instant);
        std::mem::swap(&mut self.end_instant, &mut self.next_end_instant);
        while !self.next_end_instant.is_empty() {
            self.next_end_instant.pop().call_box(self, ());
        }

        (!self.current_instant.is_empty())
            || (!self.end_instant.is_empty())
            || (!self.next_end_instant.is_empty())
    }
}

impl Runtime for ParallelRuntime {
    fn on_current_instant(&mut self, c: Box<Continuation<()>>) {
        self.current_instant.push(c);
    }

    fn on_next_instant(&mut self, c: Box<Continuation<()>>) {
        self.next_current_instant.push(c);
    }

    fn on_end_of_instant(&mut self, c: Box<Continuation<()>>) {
        self.end_instant.push(c);
    }
}