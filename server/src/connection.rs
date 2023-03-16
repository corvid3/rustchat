use convos::{bytes, decode_client_question, ServerTell};
use tokio::{
  io::AsyncReadExt,
  net::tcp::{OwnedReadHalf, OwnedWriteHalf},
  select,
  sync::{
    mpsc::{Receiver, Sender},
    watch,
  },
};

// represents a single connection to the server, does not contain client information
#[derive(Debug)]
pub struct ConnectionHandle {
  // the uid associated with this connection
  pub uid: u64,

  pub to_connection: Sender<ServerTell>,
  pub kill: watch::Sender<()>,
}

#[derive(Debug)]
pub struct ClientQuestion {
  pub data: convos::ClientQuestion,
  pub uid: u64,
}

pub async fn read_worker(
  mut kill: watch::Receiver<()>,
  uid: u64,
  stream: OwnedReadHalf,
  to_server: Sender<ClientQuestion>,
) {
  struct ReadWorker {
    uid: u64,
    stream: OwnedReadHalf,
    to_server: Sender<ClientQuestion>,
  }

  impl ReadWorker {
    async fn logic(&mut self) {
      let len = self.stream.read_u16().await.unwrap();
      dbg!(len);

      let mut buf = vec![0; len as usize];
      self.stream.read_exact(buf.as_mut_slice()).await.unwrap();

      let Some(data) = decode_client_question(buf) else {return};

      self
        .to_server
        .send(ClientQuestion {
          data,
          uid: self.uid,
        })
        .await
        .unwrap();
    }
  }

  let mut worker = ReadWorker {
    uid,
    stream,
    to_server,
  };

  loop {
    select! {
      _ = worker.logic() => {},
      _ = kill.changed() => {
        dbg!("Read got kill.");
        return
      },
    }
  }
}

pub async fn write_worker(
  kill: watch::Receiver<()>,
  stream: OwnedWriteHalf,
  from_server: Receiver<ServerTell>,
) {
}
