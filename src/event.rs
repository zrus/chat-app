use libp2p::dcutr;
use libp2p::gossipsub::GossipsubEvent;
use libp2p::identify::IdentifyEvent;

use libp2p::kad::KademliaEvent;
use libp2p::mdns::MdnsEvent;
use libp2p::ping::PingEvent;
use libp2p::relay::v2::client;

#[derive(Debug)]
pub enum Event {
  Ping(PingEvent),
  Identify(IdentifyEvent),
  Relay(client::Event),
  Dcutr(dcutr::behaviour::Event),
  Gossipsub(GossipsubEvent),
  Mdns(MdnsEvent),
  Kademlia(KademliaEvent),
}

impl From<PingEvent> for Event {
  fn from(e: PingEvent) -> Self {
    Event::Ping(e)
  }
}

impl From<IdentifyEvent> for Event {
  fn from(e: IdentifyEvent) -> Self {
    Event::Identify(e)
  }
}

impl From<client::Event> for Event {
  fn from(e: client::Event) -> Self {
    Event::Relay(e)
  }
}

impl From<dcutr::behaviour::Event> for Event {
  fn from(e: dcutr::behaviour::Event) -> Self {
    Event::Dcutr(e)
  }
}

impl From<GossipsubEvent> for Event {
  fn from(e: GossipsubEvent) -> Self {
    Event::Gossipsub(e)
  }
}

impl From<MdnsEvent> for Event {
  fn from(e: MdnsEvent) -> Self {
    Event::Mdns(e)
  }
}

impl From<KademliaEvent> for Event {
  fn from(e: KademliaEvent) -> Self {
    Event::Kademlia(e)
  }
}
