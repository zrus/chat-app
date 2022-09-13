use std::str::FromStr;

use clap::Parser;
use libp2p::{core::Multiaddr, PeerId};

#[derive(Debug, Parser)]
#[clap(name = "Chat app p2p")]
pub struct Opts {
  /// The mode (client-listen, client-dial).
  #[clap(short, long, default_value = "listen")]
  pub mode: Mode,

  /// The listening address
  #[clap(
    long,
    default_value = "/ip4/3.19.56.240/tcp/4001/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN"
  )]
  pub relay_address: Multiaddr,

  /// Peer ID of the remote peer to hole punch to.
  #[clap(short, long)]
  pub remote_peer_id: Option<PeerId>,

  /// Bootstrap ID of the bootstrap node.
  #[clap(
    short,
    long,
    default_value = "/ip4/3.19.56.240/tcp/4003/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN/p2p-circuit/p2p/12D3KooWDfVV2caaXhXPsZti1wyZPtBj7kckpQ62oSCS3vxJuzyY"
  )]
  pub bootstrap_address: Multiaddr,
}

#[derive(Debug, Parser, PartialEq)]
pub enum Mode {
  Dial,
  Listen,
}

impl FromStr for Mode {
  type Err = String;
  fn from_str(mode: &str) -> Result<Self, Self::Err> {
    match mode {
      "dial" => Ok(Mode::Dial),
      "listen" => Ok(Mode::Listen),
      _ => Err("Expected either 'dial' or 'listen'".to_string()),
    }
  }
}
