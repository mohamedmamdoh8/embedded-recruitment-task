use crate::message::{
    client_message, server_message, AddRequest, AddResponse, ClientMessage, EchoMessage, ServerMessage,
};
use log::{error, info, warn};
use prost::Message;
use std::{
    io::{self, ErrorKind, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

// Represents a client connection
struct Client {
    stream: TcpStream,
}

impl Client {
    pub fn new(stream: TcpStream) -> Self {
        Client { stream }
    }

    /// Handles incoming messages from the client
    pub fn handle(&mut self) -> io::Result<()> {
        let mut buffer = [0; 512];

        // Read data from the client
        let bytes_read = match self.stream.read(&mut buffer) {
            Ok(0) => {
                info!("Client disconnected.");
                if let Err(e) = self.stream.shutdown(Shutdown::Both) {
                    error!("Failed to shut down connection: {}", e);
                }
                return Ok(());
            }
            // to handle would block
            Ok(n) => n,
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(e) => {
                error!("Error reading from client: {}", e);
                return Err(e);
            }
        };

        // Decode the incoming message
        match ClientMessage::decode(&buffer[..bytes_read]) {
            Ok(decoded_message) => match decoded_message.message {
                Some(client_message::Message::AddRequest(addreq)) => {
                    info!("Received AddRequest: a = {}, b = {}", addreq.a, addreq.b);

                    // Create and send AddResponse
                    let add_response = ServerMessage {
                        message: Some(server_message::Message::AddResponse(AddResponse {
                            result: addreq.a + addreq.b,
                        })),
                    };

                    let mut response_buffer = Vec::new();
                    if add_response.encode(&mut response_buffer).is_ok() {
                        self.stream.write_all(&response_buffer)?;
                        self.stream.flush()?;
                        info!("Sent AddResponse: result = {}", addreq.a + addreq.b);
                    }
                }
                Some(client_message::Message::EchoMessage(echo_message)) => {
                    println!("Received EchoMessage: {}", echo_message.content);

                    // Echo back the received message
                    let echo_response = ServerMessage {
                        message: Some(server_message::Message::EchoMessage(EchoMessage {
                            content: echo_message.content,
                        })),
                    };

                    let mut response_buffer = Vec::new();
                    if echo_response.encode(&mut response_buffer).is_ok() {
                        self.stream.write_all(&response_buffer)?;
                        self.stream.flush()?;
                        info!("Echoed message back to client.");
                    }
                }
                None => println!("Received an empty or invalid message."),
            },
            Err(e) => {
                error!("Failed to decode message: {:?}", e);
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Decoding failed"));
            }
        }
        Ok(())
    }
}

pub struct Server {
    listener: TcpListener,
    is_running: Arc<AtomicBool>,
}

impl Server {
    /// Creates a new server instance
    pub fn new(addr: &str) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        let is_running = Arc::new(AtomicBool::new(false));
        Ok(Server { listener, is_running })
    }

    /// Runs the server, listening for incoming connections and handling them
    pub fn run(&self) -> io::Result<()> {
        self.is_running.store(true, Ordering::SeqCst);
        info!("Server is running on {}", self.listener.local_addr()?);

        self.listener.set_nonblocking(true)?;

        while self.is_running.load(Ordering::SeqCst) {
            match self.listener.accept() {
                Ok((stream, addr)) => {
                    println!("New client connected: {}", addr);
                    let is_running = Arc::clone(&self.is_running);

                    thread::spawn(move || {
                        let mut client = Client::new(stream);
                        while is_running.load(Ordering::SeqCst) {
                            if let Err(e) = client.handle() {
                                println!("Error handling client: {}", e);
                                break;
                            }
                        }
                    });
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    error!("Error accepting connection: {}", e);
                }
            }
        }

        info!("Server stopped.");
        self.stop();
        Ok(())
    }

    /// Stops the server by setting the `is_running` flag to `false`
    pub fn stop(&self) {
        if self.is_running.load(Ordering::SeqCst) {
            self.is_running.store(false, Ordering::SeqCst);
            info!("Shutdown signal sent.");
        } else {
            warn!("Server was already stopped or not running.");
        }
    }
}

