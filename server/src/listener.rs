use std::collections::hash_map::RandomState;

use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::{TcpListener, TcpStream, ToSocketAddrs},
  select,
  sync::{
    mpsc::{self, Receiver, Sender},
    watch,
  },
};

use crate::connection::{read_worker, write_worker, ClientQuestion, ConnectionHandle};

pub fn create_listener<T>(
  on: T,
  killswitch: watch::Receiver<()>,
  question_to_server: Sender<ClientQuestion>,
) -> Receiver<ConnectionHandle>
where
  T: ToSocketAddrs + Send + 'static,
{
  let (tx, rx) = mpsc::channel(32);

  tokio::spawn(listener_logic(killswitch, on, tx, question_to_server));

  rx
}

async fn listener_logic<T>(
  mut killswitch: watch::Receiver<()>,
  on: T,
  to_server: Sender<ConnectionHandle>,
  question_to_server: Sender<ClientQuestion>,
) where
  T: ToSocketAddrs,
{
  let listener = TcpListener::bind(on).await.unwrap();

  loop {
    select! {
      _ = killswitch.changed() => break,
      Ok((con, _ip)) = listener.accept() => {
        let to_server = to_server.clone();
        let question_to_server = question_to_server.clone();
        tokio::spawn(
          listener_accepted( con, to_server, question_to_server)
        );
      }
    }
  }
}

async fn listener_accepted(
  mut con: TcpStream,
  to_server: Sender<ConnectionHandle>,
  question_to_server: Sender<ClientQuestion>,
) {
  let num: u64 = rand::random();
  con.write_u64(num).await.unwrap();
  if con.read_u64().await.unwrap() != num {
    return;
  }

  // do a simple "what is ur uid" ask
  let uid = con.read_u64().await.unwrap();

  // spawn the workers required for the connection
  let (read, write) = con.into_split();
  let (s2c_tx, s2c_rx) = mpsc::channel(8);
  let (ks_tx, ks_rx) = watch::channel(());

  tokio::spawn(read_worker(ks_rx.clone(), uid, read, question_to_server));
  tokio::spawn(write_worker(ks_rx, write, s2c_rx));

  to_server
    .send(ConnectionHandle {
      uid,
      to_connection: s2c_tx,
      kill: ks_tx,
    })
    .await
    .unwrap();
}
