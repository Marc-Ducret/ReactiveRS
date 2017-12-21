extern crate crossbeam;

use super::*;
use self::crossbeam::sync::MsQueue;
use self::std::sync::Condvar;

//  ____            ____              _   _
// |  _ \ __ _ _ __|  _ \ _   _ _ __ | |_(_)_ __ ___   ___
// | |_) / _` | '__| |_) | | | | '_ \| __| | '_ ` _ \ / _ \
// |  __/ (_| | |  |  _ <| |_| | | | | |_| | | | | | |  __/
// |_|   \__,_|_|  |_| \_\\__,_|_| |_|\__|_|_| |_| |_|\___|

pub struct TodoQueue {
    queue: MsQueue<Box<Continuation<()>>>,
    count: Arc<Mutex<i32>>,
    notify: Condvar,
}

impl TodoQueue {
    fn new() -> Self {
        TodoQueue {
            queue: MsQueue::new(),
            count: Arc::new(Mutex::new(0)),
            notify: Condvar::new()
        }
    }

    fn push(&self, elem: Box<Continuation<()>>) {
        let mut ct = self.count.lock().unwrap();
        *ct = *ct + 1;
        self.queue.push(elem);
    }

    fn pop(&self) -> Box<Continuation<()>> {
        self.queue.pop()
    }

    fn done(&self) {
        let mut ct = self.count.lock().unwrap();
        *ct = *ct - 1;
        self.notify.notify_one();
    }

    fn is_active(&self) -> bool {
        *(self.count.lock().unwrap()) > 0
    }
}

pub struct ParallelRuntime {
    current_instant: MsQueue<Box<Continuation<()>>>,
    end_instant: MsQueue<Box<Continuation<()>>>,
    next_current_instant: MsQueue<Box<Continuation<()>>>,
    todo: TodoQueue,
    worker_count: usize,
}

impl ParallelRuntime {
    pub fn new(worker_count: usize) -> Self {
        ParallelRuntime {
            current_instant: MsQueue::new(),
            end_instant: MsQueue::new(),
            next_current_instant: MsQueue::new(),
            todo: TodoQueue::new(),
            worker_count,
        }
    }
}

impl ParallelRuntime {
    pub fn execute(self) {
        let mut workers = Vec::with_capacity(self.worker_count);
        let runtime = Arc::new(self);
        for _ in 0..runtime.worker_count {
            let runtime = runtime.clone();
            let worker = move|| {
                loop {
                    runtime.todo.pop().call_box(&mut LocalParallelRuntime {runtime: runtime.clone()}, ());
                    runtime.todo.done();
                }
            };
            workers.push(thread::spawn(worker));
        }
        while runtime.instant() {}
    }

    fn instant(&self) -> bool {
        assert!(!self.todo.is_active());
        while !self.current_instant.is_empty() {
            self.todo.push(self.current_instant.pop());
        }
        {
            let mut ct = self.todo.count.lock().unwrap();
            while *ct > 0 {
                while !self.current_instant.is_empty() {
                    self.todo.push(self.current_instant.pop());
                }
                ct = self.todo.notify.wait(ct).unwrap();
            }
        }
        while !self.end_instant.is_empty() {
            self.todo.push(self.end_instant.pop());
        }
        while !self.next_current_instant.is_empty() {
            self.current_instant.push(self.next_current_instant.pop());
        }
        {
            let mut ct = self.todo.count.lock().unwrap();
            while *ct > 0 {
                ct = self.todo.notify.wait(ct).unwrap();
            }
        }
        !(self.current_instant.is_empty() && self.end_instant.is_empty() && self.next_current_instant.is_empty())
    }

    pub fn on_current_instant(&self, c: Box<Continuation<()>>) {
        self.current_instant.push(c);
    }

    fn on_next_instant(&self, c: Box<Continuation<()>>) {
        self.next_current_instant.push(c);
    }

    fn on_end_of_instant(&self, c: Box<Continuation<()>>) {
        self.end_instant.push(c);
    }
}

pub struct LocalParallelRuntime {
    runtime: Arc<ParallelRuntime>
}

impl Runtime for LocalParallelRuntime {
    fn on_current_instant(&mut self, c: Box<Continuation<()>>) {
        self.runtime.on_current_instant(c);
    }

    fn on_next_instant(&mut self, c: Box<Continuation<()>>) {
        self.runtime.on_next_instant(c);
    }

    fn on_end_of_instant(&mut self, c: Box<Continuation<()>>) {
        self.runtime.on_end_of_instant(c);
    }
}