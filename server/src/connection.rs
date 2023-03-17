use std::{
  cell::Cell,
  sync::{Arc, RwLock},
};

use convos::{bytes, decode_client_question, encode_server_question, ServerTell};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::tcp::{OwnedReadHalf, OwnedWriteHalf},
  select,
  sync::{
    broadcast,
    mpsc::{self, Receiver, Sender},
    watch,
  },
};

pub(crate) type UID = u64;
pub(crate) type ConID = u64;

// represents a single connection to the server, does not contain client information
#[derive(Debug, Clone)]
pub struct ConnectionHandle {
  pub to_connection: Sender<ServerTell>,
  // update the UID of this connection
  pub update_uid: mpsc::Sender<u64>,
  pub kill: broadcast::Sender<()>,
}

#[derive(Debug)]
pub struct ClientQuestion {
  pub data: convos::ClientQuestion,

  // include both the con_id and the uid
  // each concurrent connection will get a unique ID along with
  //     the users UID
  // if the user is not logged in, UID = 0, and thus any question will be
  // indistinguishable from any other anonymous question
  // thus, for when an anonymous user logs in, we need to discern between the connections
  //   then apply the related UID to the cell, and start differentiating that way
  pub con_id: ConID,
  pub uid: UID,
}

pub async fn read_worker(
  mut kill: broadcast::Receiver<()>,
  update_uid: mpsc::Receiver<u64>,
  con_id: ConID,
  stream: OwnedReadHalf,
  to_server: Sender<ClientQuestion>,
) {
  struct ReadWorker {
    update_uid: mpsc::Receiver<u64>,
    uid: UID,
    con_id: ConID,
    stream: OwnedReadHalf,
    to_server: Sender<ClientQuestion>,
  }

  impl ReadWorker {
    async fn logic(&mut self) {
      // we can only allow the uid to update before any message is received,
      //  not /while/ a message is being received, this is why we do not have the uid update
      //  in the outer loop/select
      let len = select! {
        Ok(len) = self.stream.read_u16() => len,
        Some(id) = self.update_uid.recv() => {
          self.uid = id;
          return;
        }
      };

      dbg!(len);
      if len == 0 {
        return;
      }

      let mut buf = vec![0; len as usize];
      self.stream.read_exact(buf.as_mut_slice()).await.unwrap();

      let Some(data) = decode_client_question(buf) else {return};

      self
        .to_server
        .send(ClientQuestion {
          data,
          uid: self.uid.clone(),
          con_id: self.con_id,
        })
        .await
        .unwrap();
    }
  }

  let mut worker = ReadWorker {
    update_uid,
    // all connections to the server start out anonymously,
    uid: 0,
    con_id,
    stream,
    to_server,
  };

  loop {
    select! {
      _ = worker.logic() => {},
      _ = kill.recv() => {
        dbg!("Read got kill.");
        return
      },
    }
  }
}

// subscribe to channels witin the redis database?
pub async fn write_worker(
  mut kill: broadcast::Receiver<()>,
  mut stream: OwnedWriteHalf,
  mut from_server: Receiver<ServerTell>,
) {
  loop {
    select! {
      Some(msg) = from_server.recv() => {
        stream.write_all(&encode_server_question(msg).unwrap()).await.unwrap();
      }
      _ = kill.recv() => {
        dbg!("Write got kill.");
        return
      }
    }
  }
}
