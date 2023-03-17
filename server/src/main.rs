mod connection;
mod listener;

use std::{
  cell::Cell,
  collections::HashMap,
  sync::{Arc, RwLock},
};

use connection::{read_worker, write_worker, ClientQuestion, ConnectionHandle, UID};
use convos::ServerTell;
use listener::create_listener;
use rand::{distributions::Alphanumeric, Rng};
use sha2::{Digest, Sha512};
use sqlx::{pool::PoolConnection, PgPool, Postgres};
use tokio::{
  net::TcpStream,
  select,
  sync::{
    broadcast,
    mpsc::{self, Receiver, Sender},
    watch,
  },
};

// contains client information that is stored on the server
struct Client {
  name: String,
  uid: UID,
}

//# passowrd server, for verifying passwords on attempt to connect?

struct Server {
  connections: HashMap<u64, ConnectionHandle>,
  database: PgPool,

  // incoming tcpstreams from the listener
  // the listener will have already performed a handshake at this point,
  // all the server has to do is create the worker tasks & the unique connection ID
  listener: Receiver<TcpStream>,

  // keep a copy of the sender alive, so that it may be copied into
  // new connections, and if all of the connections close, the channel does not close
  incoming_questions: Receiver<ClientQuestion>,
  incoming_question_tx: Sender<ClientQuestion>,

  killswitch: watch::Sender<()>,

  // a copy of the killswitch-receiver, pass this to
  // all subordinate tasks to kill when server is ready to die
  killswitch_receiver: watch::Receiver<()>,
}

impl Server {
  async fn new() -> Self {
    let (ks_tx, ks_rx) = watch::channel(());
    let (iq_tx, iq_rx) = mpsc::channel(256);

    let pool = PgPool::connect("postgresql://postgres@localhost/rustChatUsers")
      .await
      .unwrap();

    Self {
      database: pool,
      connections: HashMap::new(),
      listener: create_listener("0.0.0.0:5555", ks_rx.clone()),
      incoming_questions: iq_rx,
      incoming_question_tx: iq_tx,
      killswitch: ks_tx,
      killswitch_receiver: ks_rx,
    }
  }

  async fn run(mut self) {
    // let mut heartbeat_interval = tokio::time::interval(tokio::time::Duration::from_secs(5));

    loop {
      select! {
        Some(incoming) = self.listener.recv() => self.on_incoming(incoming),
        Some(message) = self.incoming_questions.recv() => {
          tokio::spawn(
            message_worker(
              self.database.acquire().await.unwrap(),
              self.connections.get(&message.con_id).unwrap().clone(),
              message
            )
          );
        },
        // _ = heartbeat_interval.tick() => self.heartbeat_and_prune(),
      }
    }
  }

  fn heartbeat_and_prune(&mut self) {
    unimplemented!()
  }

  // TODO: put this into a worker function
  fn on_incoming(&mut self, con: TcpStream) {
    // create the unique connection ID
    let conid = loop {
      let test: u64 = rand::random();
      if !self.connections.contains_key(&test) {
        break test;
      };
    };

    let (read, write) = con.into_split();
    let (s2c_tx, s2c_rx) = mpsc::channel(8);
    let (ks_tx, _keepalive) = broadcast::channel(1);
    let (uid_tx, uid_rx) = mpsc::channel(1);

    tokio::spawn(read_worker(
      ks_tx.subscribe(),
      uid_rx,
      conid,
      read,
      self.incoming_question_tx.clone(),
    ));

    tokio::spawn(write_worker(ks_tx.subscribe(), write, s2c_rx));

    let handle = ConnectionHandle {
      update_uid: uid_tx,
      to_connection: s2c_tx,
      kill: ks_tx,
    };

    self.connections.insert(conid, handle);
  }
}

async fn message_worker(
  db: PoolConnection<Postgres>,
  connection: ConnectionHandle,
  msg: ClientQuestion,
) {
  dbg!("{:?}", &msg);

  let msg = if msg.uid == 0 {
    anonymous_message_worker(db, &connection, msg).await
  } else {
    signed_in_message_worker(db, &connection, msg).await
  };

  connection.to_connection.send(msg).await.unwrap();
}

async fn anonymous_message_worker(
  mut db: PoolConnection<Postgres>,
  connection: &ConnectionHandle,
  msg: ClientQuestion,
) -> ServerTell {
  match msg.data {
    convos::ClientQuestion::WhoIsID { id } => todo!(),

    convos::ClientQuestion::WhoAmI => ServerTell::Who {
      id: 0,
      name: "Anonymous".to_owned(),
    },

    convos::ClientQuestion::SignUp { username, password } => {
      // check if the username is already taken
      let uname_query = sqlx::query("select uid from users where name='$1'")
        .bind(&username)
        .fetch_one(&mut db)
        .await;

      if uname_query.is_ok() {
        return ServerTell::Error(convos::Error::UsernameTaken);
      };

      // generate a salt for the pass
      let salt: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(5)
        .map(char::from)
        .collect();
      dbg!(&salt);

      // create the hashed password
      let password = password + &salt;
      let mut hasher = Sha512::new();
      hasher.update(password);
      let hash: Vec<u8> = hasher.finalize().to_vec();

      // generate a unique id
      let uid = loop {
        let uid: i64 = rand::random();
        if sqlx::query("select uid from users where uid=$1")
          .bind(uid)
          .fetch_one(&mut db)
          .await
          .is_err()
        {
          break uid;
        }
      };

      if let Err(e) = sqlx::query("insert into users values ($1, $2, $3, $4)")
        .bind(uid)
        .bind(&username)
        .bind(&salt)
        .bind(&hash)
        .execute(&mut db)
        .await
      {
        eprintln!("Ran into error when trying to insert a new user into the database {e}");
      }

      ServerTell::Success(convos::Success::SignUp)
    }

    convos::ClientQuestion::SignIn { id, password } => todo!(),
    convos::ClientQuestion::WhoIsName { name } => todo!(),
  }
}

async fn signed_in_message_worker(
  db: PoolConnection<Postgres>,
  connection: &ConnectionHandle,
  msg: ClientQuestion,
) -> ServerTell {
  match msg.data {
    convos::ClientQuestion::WhoIsID { id } => todo!(),
    convos::ClientQuestion::WhoIsName { name } => todo!(),
    convos::ClientQuestion::WhoAmI => todo!(),
    convos::ClientQuestion::SignUp { username, password } => todo!(),
    convos::ClientQuestion::SignIn { id, password } => todo!(),
  }
}

async fn startup_tasks() {}

async fn inner_main() {
  startup_tasks().await;
  Server::new().await.run().await;
}

#[tokio::main]
async fn main() {
  inner_main().await;
}
