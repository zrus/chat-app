use crate::event::Event;
use libp2p::dcutr;
use libp2p::gossipsub::Gossipsub;
use libp2p::kad::store::MemoryStore;
use libp2p::kad::Kademlia;
use libp2p::mdns::Mdns;
use libp2p::ping::Ping;
use libp2p::relay::v2::client::Client;
use libp2p::{identify::Identify, NetworkBehaviour};

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "Event", event_process = false)]
pub struct Behaviour {
  pub client: Client,
  pub ping: Ping,
  pub identify: Identify,
  pub dcutr: dcutr::behaviour::Behaviour,
  pub kademlia: Kademlia<MemoryStore>,
  pub gossipsub: Gossipsub,
  pub mdns: Mdns,
}
