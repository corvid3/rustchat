use std::{io::Read, mem::transmute};

pub mod bytes {
  pub const OK: u8 = 0x00;
  pub const WHO_IS: u8 = 0x10;
  pub const WHOAMI: u8 = 0x11;

  pub const HEARTBEAT: u8 = 0xFF;
  pub const SYNDICATION: u8 = 0xA0;
}

pub enum ServerTell {
  NumConnected,

  // response to a WhoIs packet
  Who { id: u64, name: String },
  Syndication { from: u64, content: String },
}

#[derive(Debug)]
pub enum ClientQuestion {
  WhoIs { id: u64 },
  WhoAmI,
}

pub fn decode_client_question(vec: Vec<u8>) -> Option<ClientQuestion> {
  match vec[0] {
    bytes::WHO_IS => {
      let num = u64::from_be_bytes(vec[1..9].try_into().unwrap());
      Some(ClientQuestion::WhoIs { id: num })
    }

    bytes::WHOAMI => Some(ClientQuestion::WhoAmI),

    _ => {
      dbg!("Ran into unknown type clarifier");
      None
    }
  }
}

pub fn encode_client_question(question: ClientQuestion) -> Option<Vec<u8>> {
  let vec: Vec<u8> = match question {
    ClientQuestion::WhoIs { id } => {
      let mut vec = vec![bytes::WHO_IS];
      vec.extend_from_slice(&id.to_be_bytes());
      vec
    }

    ClientQuestion::WhoAmI => vec![bytes::WHOAMI],
  };

  let vec_size = vec.len();
  if vec_size > u16::MAX as usize {
    return None;
  }

  let mut vec2 = Vec::new();
  vec2.extend_from_slice(&(vec_size as u16).to_be_bytes());
  vec2.extend(vec);
  Some(vec2)
}

impl ServerTell {
  // decodes a stream of bytes into a usable tell
  fn decode<T>(input: T)
  where
    T: IntoIterator<Item = u8>,
  {
    let x = input.into_iter();
  }
}
