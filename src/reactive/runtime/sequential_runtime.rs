use super::*;

//  ____             ____              _   _
// / ___|  ___  __ _|  _ \ _   _ _ __ | |_(_)_ __ ___   ___
// \___ \ / _ \/ _` | |_) | | | | '_ \| __| | '_ ` _ \ / _ \
//  ___) |  __/ (_| |  _ <| |_| | | | | |_| | | | | | |  __/
// |____/ \___|\__, |_| \_\\__,_|_| |_|\__|_|_| |_| |_|\___|
//                |_|


pub struct SequentialRuntime {
    current_instant: Vec<Box<Continuation<()>>>,
    end_instant: Vec<Box<Continuation<()>>>,
    next_current_instant: Vec<Box<Continuation<()>>>,
    next_end_instant: Vec<Box<Continuation<()>>>,
}

impl SequentialRuntime {
    pub fn new() -> Self {
        SequentialRuntime {
            current_instant: Vec::new(),
            end_instant: Vec::new(),
            next_current_instant: Vec::new(),
            next_end_instant: Vec::new(),
        }
    }
}

impl Runtime for SequentialRuntime {
    fn execute(&mut self) {
        while self.instant() {}
    }

    fn instant(&mut self) -> bool {
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