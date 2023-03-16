mod connection;
mod listener;

use std::collections::HashMap;

use connection::{ClientQuestion, ConnectionHandle};
use listener::create_listener;
use tokio::{
  net::TcpStream,
  select,
  sync::{
    mpsc::{self, Receiver, Sender},
    watch,
  },
};

// contains client information that is stored on the server
struct Client {
  name: String,
  uid: u64,
}

// contains information about clients
struct ClientServer {}
struct ClientServerHandle {}

//# passowrd server, for verifying passwords on attempt to connect?

struct Server {
  clients: HashMap<u64, Client>,
  connections: HashMap<u64, ConnectionHandle>,
  listener: Receiver<ConnectionHandle>,

  incoming_questions: Receiver<ClientQuestion>,

  killswitch: watch::Sender<()>,

  // a copy of the killswitch-receiver, pass this to
  // all subordinate tasks to kill when server is ready to die
  killswitch_receiver: watch::Receiver<()>,
}

impl Server {
  fn new() -> Self {
    let (ks_tx, ks_rx) = watch::channel(());
    let (iq_tx, iq_rx) = mpsc::channel(256);

    Self {
      clients: HashMap::new(),
      connections: HashMap::new(),
      listener: create_listener("0.0.0.0:5555", ks_rx.clone(), iq_tx),
      incoming_questions: iq_rx,
      killswitch: ks_tx,
      killswitch_receiver: ks_rx,
    }
  }

  async fn run(mut self) {
    let mut heartbeat_interval = tokio::time::interval(tokio::time::Duration::from_secs(5));

    loop {
      select! {
        Some(incoming) = self.listener.recv() => self.on_incoming(incoming),
        Some(message) = self.incoming_questions.recv() => self.on_message(message),
        // _ = heartbeat_interval.tick() => self.heartbeat_and_prune(),
      }
    }
  }

  fn heartbeat_and_prune(&mut self) {
    unimplemented!()
  }

  fn on_incoming(&mut self, handle: ConnectionHandle) {
    _ = self.connections.insert(handle.uid, handle);
  }

  fn on_message(&mut self, msg: ClientQuestion) {
    dbg!("{:?}", msg);
  }
}

fn startup_tasks() {}

async fn inner_main() {
  startup_tasks();
  Server::new().run().await;
}

#[tokio::main]
async fn main() {
  inner_main().await;
}
