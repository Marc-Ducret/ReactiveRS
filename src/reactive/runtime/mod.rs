use super::*;

pub mod sequential_runtime;
pub mod parallel_runtime;

//  ____              _   _
// |  _ \ _   _ _ __ | |_(_)_ __ ___   ___
// | |_) | | | | '_ \| __| | '_ ` _ \ / _ \
// |  _ <| |_| | | | | |_| | | | | | |  __/
// |_| \_\\__,_|_| |_|\__|_|_| |_| |_|\___|

pub trait Runtime: Send {
    fn on_current_instant(&mut self, c: Box<Continuation<()>>);

    fn on_next_instant(&mut self, c: Box<Continuation<()>>);

    fn on_end_of_instant(&mut self, c: Box<Continuation<()>>);
}