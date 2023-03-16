use convos::{encode_client_question, ClientQuestion};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::{
    tcp::{OwnedReadHalf, OwnedWriteHalf},
    TcpStream,
  },
  select,
  sync::{mpsc, watch},
};

pub struct BrokerHandle {
  pub from_broker: mpsc::Receiver<String>,
  pub to_broker: mpsc::Sender<String>,
}

struct Workers {
  kill: watch::Sender<()>,
  to_server: mpsc::Sender<Vec<u8>>,
  from_server: mpsc::Receiver<Vec<u8>>,
}

/// mediates the conversion of inputted commands to ClientQuestions,
/// and the communictaions between the client interface and the server
pub struct Broker {
  // the uid of this connection and current surver
  my_id: u64,
  to_handle: mpsc::Sender<String>,
  from_handle: mpsc::Receiver<String>,
  workers: Option<Workers>,
}

/// user->broker command
pub enum Command {
  Ping,
  WhoAmI,
  Connect(String),
  Disconnect,
  Message(String),
  SetId(u64),
  Unknown,
  Error(String),
}

mod command_parsing {
  use logos::Logos;

  use super::Command;

  #[derive(Logos, Debug, PartialEq)]
  enum Token {
    #[regex(r"[a-zA-Z0-9\.:]+")]
    Identifier,

    #[regex("\"[a-zA-Z]+\"")]
    String,
    #[error]
    #[regex(r"[ \t\n\f]+", logos::skip)]
    Error,
  }

  pub fn parse(str: String) -> Command {
    if str.starts_with("//") || !str.starts_with("/") {
      Command::Message(str)
    } else {
      let mut lex = Token::lexer(&str);

      lex.next();
      lex.next();
      match lex.slice() {
        "ping" => Command::Ping,
        "whoami" => Command::WhoAmI,
        "connect" => {
          if lex.next().is_none() {
            return Command::Error("Expected an address after /connect".to_owned());
          }

          Command::Connect(lex.slice().to_owned())
        }
        "setid" => {
          if lex.next().is_none() {
            return Command::Error("Expected an id after /setid".to_owned());
          }

          let Ok(id) = str::parse::<u64>(lex.slice()) else {
            return Command::Error("Not a u64 after /setid".to_owned());
          };

          Command::SetId(id)
        }
        "disconnect" => Command::Disconnect,
        _ => Command::Unknown,
      }
    }
  }
}

impl Broker {
  pub fn new() -> BrokerHandle {
    let (th_tx, th_rx) = mpsc::channel(256);
    let (fh_tx, fh_rx) = mpsc::channel(256);

    std::thread::spawn(|| {
      tokio::runtime::Runtime::new().unwrap().block_on(
        Self {
          my_id: 0,
          to_handle: th_tx,
          from_handle: fh_rx,
          workers: None,
        }
        .logic(),
      );
    });

    BrokerHandle {
      from_broker: th_rx,
      to_broker: fh_tx,
    }
  }

  // could probably make this better, just would need to do some redesigning
  async fn logic(mut self) {
    loop {
      if let Some(Workers { from_server, .. }) = &mut self.workers {
        select! {
          Some(msg) = from_server.recv() => self.handle_incoming_from_server(msg).await,
          Some(msg) = self.from_handle.recv() => self.handle_incoming_from_user(msg).await,
        };
      } else {
        let i = self.from_handle.recv().await.unwrap();
        self.handle_incoming_from_user(i).await;
      }
    }
  }

  async fn disconnect(&mut self) {
    if let Some(workers) = &mut self.workers {
      workers.kill.send(()).unwrap();

      workers.to_server.closed().await;
      workers.from_server.close();
    }

    self.workers = None;
  }

  async fn handle_incoming_from_server(&mut self, msg: Vec<u8>) {}
  async fn handle_incoming_from_user(&mut self, msg: String) {
    let cmd = command_parsing::parse(msg);

    match cmd {
      Command::Ping => self.to_handle.send("Pong!".to_string()).await.unwrap(),

      Command::Connect(ref addr) => {
        self.disconnect().await;

        // try to connect to the server
        dbg!(addr.clone());
        let Ok(mut stream) = TcpStream::connect(addr).await else {
          self.to_handle.send("Invalid address.".to_owned()).await.unwrap();
          return;
        };

        // give the handshake to the server
        let hs = stream.read_u64().await.unwrap();
        stream.write_u64(hs).await.unwrap();

        stream.write_u64(self.my_id).await.unwrap();

        let (read_half, write_half) = stream.into_split();

        // create a new set of workers
        let (read_tx, read_rx) = mpsc::channel(256);
        let (write_tx, write_rx) = mpsc::channel(256);
        let (kill_tx, kill_rx) = watch::channel(());

        tokio::spawn(reader(kill_rx.clone(), read_half, read_tx));

        tokio::spawn(writer(kill_rx, write_half, write_rx));

        self.workers = Some(Workers {
          kill: kill_tx,
          to_server: write_tx,
          from_server: read_rx,
        });

        self
          .to_handle
          .send(format!("Connected to: {}", addr))
          .await
          .unwrap();
      }

      Command::WhoAmI => {
        let Some(workers) = &self.workers else {
          self.to_handle.send("Cannot whoami when not connected.".to_owned()).await.unwrap();
          return;
        };

        workers
          .to_server
          .send(encode_client_question(ClientQuestion::WhoAmI).unwrap())
          .await
          .unwrap();

        self.to_handle.send("Sent whoami".to_owned()).await.unwrap();
      }

      Command::SetId(id) => {
        if self.workers.is_some() {
          self
            .to_handle
            .send("Cannot change ID while connected to a server.".to_owned())
            .await
            .unwrap();
          return;
        }

        self.my_id = id;
      }

      Command::Disconnect => {
        if self.workers.is_none() {
          self
            .to_handle
            .send("Already disconnected.".to_owned())
            .await
            .unwrap();
        } else {
          self.disconnect().await;
          self
            .to_handle
            .send("Disconnected from server".to_owned())
            .await
            .unwrap();
        }
      }

      Command::Message(msg) => {
        if let Some(workers) = &self.workers {
          workers
            .to_server
            .send(msg.clone().into_bytes())
            .await
            .unwrap();
          self.to_handle.send(msg).await.unwrap();
        } else {
          self
            .to_handle
            .send("No server currently connected to send a message to.".to_owned())
            .await
            .unwrap();
        }
      }

      Command::Unknown => self
        .to_handle
        .send("Unknown command".to_string())
        .await
        .unwrap(),

      Command::Error(e) => self.to_handle.send(e).await.unwrap(),
    }
  }
}

async fn writer(
  mut kill: watch::Receiver<()>,
  mut stream: OwnedWriteHalf,
  mut from_broker: mpsc::Receiver<Vec<u8>>,
) {
  loop {
    select! {
      Some(msg) = from_broker.recv() => {
        dbg!(msg.len());
        stream.write_all(msg.as_slice()).await.unwrap();
      }
      _ = kill.changed() => return,
    }
  }
}

async fn reader(
  mut kill: watch::Receiver<()>,
  stream: OwnedReadHalf,
  to_broker: mpsc::Sender<Vec<u8>>,
) {
  loop {
    select! {
      _ = kill.changed() => return,
    }
  }
}
