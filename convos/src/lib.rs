use std::{fmt::Display, io::Read, mem::transmute};

use serde::{Deserialize, Serialize};

pub mod bytes {
  pub const OK: u8 = 0x00;
  pub const WHO_IS: u8 = 0x10;
  pub const WHOAMI: u8 = 0x11;

  pub const HEARTBEAT: u8 = 0xFF;
  pub const SYNDICATION: u8 = 0xA0;
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Error {
  NotLoggedIn,
  AlreadyLoggedIn,

  UsernameTaken,
  InvalidUID,
  InvalidUsername,
  InvalidPassword,
}

impl Display for Error {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(match self {
      Error::NotLoggedIn => "Not logged in",
      Error::AlreadyLoggedIn => "Already logged in",
      Error::UsernameTaken => "Username is taken",
      Error::InvalidUID => "Invalid UID",
      Error::InvalidUsername => "Invalid Username",
      Error::InvalidPassword => "Invalid password",
    })
  }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Success {
  SignIn,
  SignUp,
}

impl Display for Success {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(match self {
      Success::SignIn => "Successfully signed in",
      Success::SignUp => "Successfully signed up",
    })
  }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerTell {
  NumConnected,

  // response to a WhoIs packet
  Who { id: u64, name: String },
  Syndication { from: u64, content: String },

  Success(Success),
  Error(Error),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientQuestion {
  SignUp { username: String, password: String },
  SignIn { id: u64, password: String },
  WhoIsID { id: u64 },
  WhoIsName { name: String },
  WhoAmI,
}

pub fn encode_client_question(question: ClientQuestion) -> Option<Vec<u8>> {
  let str = serde_json::to_string(&question).ok()?;
  dbg!(str);

  let mut json = serde_json::to_vec(&question).ok()?;
  let len = json.len();

  if len > u16::MAX as usize {
    return None;
  }

  let mut vec = vec![];
  vec.extend_from_slice(&(len as u16).to_be_bytes());
  vec.append(&mut json);

  Some(vec)
}

pub fn decode_client_question(vec: Vec<u8>) -> Option<ClientQuestion> {
  let cli = serde_json::from_slice(vec.as_slice()).ok()?;
  Some(cli)
}

pub fn encode_server_question(question: ServerTell) -> Option<Vec<u8>> {
  let mut json = serde_json::to_vec(&question).ok()?;
  let len = json.len();

  if len > u16::MAX as usize {
    return None;
  }

  let mut vec = vec![];
  vec.extend_from_slice(&(len as u16).to_be_bytes());
  vec.append(&mut json);

  Some(vec)
}

pub fn decode_server_question(vec: Vec<u8>) -> Option<ServerTell> {
  let cli = serde_json::from_slice(vec.as_slice()).ok()?;
  Some(cli)
}
