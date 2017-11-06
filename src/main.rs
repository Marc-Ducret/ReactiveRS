/// A reactive continuation awaiting a value of type `V`. For the sake of simplicity,
/// continuation must be valid on the static lifetime.
pub trait Continuation<V>: 'static {
    /// Calls the continuation.
    fn call(self, runtime: &mut Runtime, value: V);

    /// Calls the continuation. Works even if the continuation is boxed.
    ///
    /// This is necessary because the size of a value must be known to unbox it. It is
    /// thus impossible to take the ownership of a `Box<Continuation>` whitout knowing the
    /// underlying type of the `Continuation`.
    fn call_box(self: Box<Self>, runtime: &mut Runtime, value: V);
}

impl<V, F> Continuation<V> for F where F: FnOnce(&mut Runtime, V) + 'static {
    fn call(self, runtime: &mut Runtime, value: V)  {
        self(runtime, value);
    }

    fn call_box(self: Box<Self>, runtime: &mut Runtime, value: V) {
        (*self).call(runtime, value);
    }
}

/// Runtime for executing reactive continuations.
pub struct Runtime {
    currentInstant : Vec<Box<Continuation<()>>>,
    endInstant : Vec<Box<Continuation<()>>>,
    nextCurrentInstant : Vec<Box<Continuation<()>>>,
    nextEndInstant : Vec<Box<Continuation<()>>>,
}

impl Runtime {
    /// Creates a new `Runtime`.
    pub fn new() -> Self {
        Runtime {
            currentInstant : Vec::new(),
            endInstant : Vec::new(),
            nextCurrentInstant : Vec::new(),
            nextEndInstant : Vec::new(),
        }
    }

    /// Executes instants until all work is completed.
    pub fn execute(&mut self) {
        while self.instant() {}
    }

    /// Executes a single instant to completion. Indicates if more work remains to be done.
    pub fn instant(&mut self) -> bool {
        println!("instant");
        while let Some(cont) = self.currentInstant.pop() {
            cont.call_box(self, ());
        }
        std::mem::swap(&mut self.currentInstant, &mut self.nextCurrentInstant);
        std::mem::swap(&mut self.endInstant, &mut self.nextEndInstant);
        while let Some(cont) = self.nextEndInstant.pop() {
            cont.call_box(self, ());
        }

        (!self.currentInstant.is_empty())
     || (!self.endInstant.is_empty())
     || (!self.nextEndInstant.is_empty())
    }

    /// Registers a continuation to execute on the current instant.
    fn on_current_instant(&mut self, c: Box<Continuation<()>>) {
        self.currentInstant.push(c);
    }

    /// Registers a continuation to execute at the next instant.
    fn on_next_instant(&mut self, c: Box<Continuation<()>>) {
        self.nextCurrentInstant.push(c);
    }

    /// Registers a continuation to execute at the end of the instant. Runtime calls for `c`
    /// behave as if they where executed during the next instant.
    fn on_end_of_instant(&mut self, c: Box<Continuation<()>>) {
        self.endInstant.push(c);
    }
}

fn question2() {
    let mut runtime = Runtime::new();
    let contPrint = Box::new(|run :&mut Runtime, ()| print!("42"));
    let contWait = Box::new(|run :&mut Runtime, ()| run.on_next_instant(contPrint));
    runtime.on_current_instant(contWait);
    runtime.execute();
}

fn main() {

}
