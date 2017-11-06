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
    // TODO
}

impl Runtime {
    /// Creates a new `Runtime`.
    pub fn new() -> Self { unimplemented!() } // TODO

    /// Executes instants until all work is completed.
    pub fn execute(&mut self) {
        unimplemented!() // TODO
    }

    /// Executes a single instant to completion. Indicates if more work remains to be done.
    pub fn instant(&mut self) -> bool {
        unimplemented!() // TODO
    }

    /// Registers a continuation to execute on the current instant.
    fn on_current_instant(&mut self, c: Box<Continuation<()>>) {
        unimplemented!() // TODO
    }

    /// Registers a continuation to execute at the next instant.
    fn on_next_instant(&mut self, c: Box<Continuation<()>>) {
        unimplemented!() // TODO
    }

    /// Registers a continuation to execute at the end of the instant. Runtime calls for `c`
    /// behave as if they where executed during the next instant.
    fn on_end_of_instant(&mut self, c: Box<Continuation<()>>) {
        unimplemented!() // TODO
    }
}

fn main() {
    println!("Hello, world!");
}
