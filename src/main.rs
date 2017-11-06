extern crate futures;
extern crate tokio_core;
extern crate tokio_io;
extern crate bytes;

use futures::unsync::mpsc;


enum Error {
    /// Error produced by an IO opperation.
    IoError(std::io::Error),
    /// Error produced by an MPSC channel.
    SendError,
    /// The client sent an invalid frame.
    InvalidFrame,
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error { Error::IoError(e) }
}

impl<T> From<mpsc::SendError<T>> for Error {
    fn from(_: mpsc::SendError<T>) -> Error { Error::SendError }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            Error::IoError(ref e) => e.fmt(f),
            Error::SendError => write!(f, "mpsc send failed"),
            Error::InvalidFrame => write!(f, "invalid frame"),
        }
    }
}

use bytes::BytesMut;
use tokio_io::{AsyncRead, codec};
use std::rc::Rc;

struct Codec;

impl codec::Decoder for Codec {
    type Item = String;
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Error> {
        if let Some(i) = buf.iter().position(|&b| b == b'\n') {
            // remove the serialized frame from the buffer.
            let line = buf.split_to(i);
            // Also remove the '\n'
            buf.split_to(1);
            // Turn this data into a UTF string and return it in a Frame.
            match std::str::from_utf8(&line) {
                Ok(s) => Ok(Some(s.to_string())),
                Err(_) => Err(Error::InvalidFrame),
            }
        } else {
            Ok(None)
        }
    }
}

impl codec::Encoder for Codec {
    type Item = Rc<String>;
    type Error = Error;

    fn encode(&mut self, msg: Rc<String>, buf: &mut BytesMut) -> Result<(), Error> {
        buf.extend(msg.as_bytes());
        buf.extend(b"\n");
        Ok(())
    }
}

use futures::{Future, Stream, Sink, stream};
use tokio_core::net::TcpStream;
use tokio_core::reactor::Core;
use tokio_core::net::TcpListener;

fn client() { //CLIENT
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let addr = "192.168.43.4:12345".parse().unwrap();
//    let addr = "127.0.0.1:12345".parse().unwrap();
    // Establish a conection to the server. `TcpStream::connect` produces a future that we
    // must resolve with the event loop.
    let socket = core.run(TcpStream::connect(&addr, &handle)).unwrap();
    // Obtain a sink and a stream to interface with the socket.
    let (writer, reader) = socket.framed(Codec).split();
    /// Create a future that prints each message to the console.
    let printer = reader.for_each(|msg| { println!("recv: {}", msg); Ok(()) });
    /// Add the future to the event loop, panic if an error is encountered.
    handle.spawn(printer.map_err(|err| panic!("{}", err)));
    /// Create a future than send 10^9 messages.
    let sender = stream::iter_ok::<_, ()>(0..1_000_000_000)
        // Convert numbers to messages
        .map(|i| Rc::new(format!("marc {}", i)))
        // Send all messages to the sink.
        .forward(writer.sink_map_err(|err| panic!("{}", err)));
    /// Spin-up the event loop until `sender` is completed.
    core.run(sender).unwrap();
}

fn server_echo() { //SERVER
    // Create the event loop that will drive this server
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    // Bind the server's socket
    let addr = "127.0.0.1:12345".parse().unwrap();
    let listener = TcpListener::bind(&addr, &handle).unwrap();

    // Handle the stream of incoming connections.
    let server = listener.incoming().for_each(|(socket, _)| {
        let (writer, reader) = socket.framed(Codec).split();
        let echo = reader.map(|msg| Rc::new(format!("SALUT {}", msg))).forward(writer).map(|_| ()).map_err(|_| ());
        handle.spawn(echo);
        Ok(())
    });

    // Spin up the server on the event loop
    core.run(server).unwrap();
}

use futures::unsync::mpsc::*;

enum BufferElement {
    Message(Rc<String>),
    Client(UnboundedSender<Rc<String>>),
}

fn server() { //SERVER
    // Create the event loop that will drive this server
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    // Bind the server's socket
    let addr = "127.0.0.1:12345".parse().unwrap();
    let listener = TcpListener::bind(&addr, &handle).unwrap();

    let mut client_buffers : Vec<UnboundedSender<Rc<String>>> = Vec::new();
    let (main_sender, main_receiver) = unbounded();

    handle.spawn(main_receiver.for_each(move |elem| {
        match elem {
            BufferElement::Message(msg) => {
                for c in &mut client_buffers {
                    c.unbounded_send(msg.clone());
                }
            },
            BufferElement::Client(buffer) => {
                client_buffers.push(buffer);
            }
        }
        Ok(())
    }));
    // Handle the stream of incoming connections.
    let server = listener.incoming().for_each(|(socket, _)| {
        let (writer, reader) = socket.framed(Codec).split();
        let (buffer_sender, buffer_receiver) = unbounded();
        main_sender.unbounded_send(BufferElement::Client(buffer_sender));
        let send = reader.map(|msg| BufferElement::Message(Rc::new(msg))).forward(main_sender.clone()).map(|_| ()).map_err(|_| ());
        handle.spawn(send);
        handle.spawn(buffer_receiver.forward(writer.sink_map_err(|_| ())).map(|_| ()).map_err(|_| ()));
        Ok(())
    });

    // Spin up the server on the event loop
    core.run(server).unwrap();
}

fn main() {
    client();
}