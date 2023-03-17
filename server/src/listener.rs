use std::{cell::Cell, collections::hash_map::RandomState, sync::Arc};

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

pub fn create_listener<T>(on: T, killswitch: watch::Receiver<()>) -> Receiver<TcpStream>
where
  T: ToSocketAddrs + Send + 'static,
{
  let (tx, rx) = mpsc::channel(32);

  tokio::spawn(listener_logic(killswitch, on, tx));

  rx
}

async fn listener_logic<T>(mut killswitch: watch::Receiver<()>, on: T, to_server: Sender<TcpStream>)
where
  T: ToSocketAddrs,
{
  let listener = TcpListener::bind(on).await.unwrap();

  loop {
    select! {
      _ = killswitch.changed() => break,
      Ok((con, _ip)) = listener.accept() => {
        let to_server = to_server.clone();
        tokio::spawn(
          listener_accepted( con, to_server)
        );
      }
    }
  }
}

async fn listener_accepted(mut con: TcpStream, to_server: Sender<TcpStream>) {
  // perform a basic handshake, requesting the echo of a u64
  let num: u64 = rand::random();
  con.write_u64(num).await.unwrap();
  if con.read_u64().await.unwrap() != num {
    return;
  }

  to_server.send(con).await.unwrap();
}
