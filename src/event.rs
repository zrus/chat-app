use libp2p::dcutr;
use libp2p::identify::IdentifyEvent;
use libp2p::kad::KademliaEvent;
use libp2p::ping::PingEvent;
use libp2p::relay::v2::client;

#[derive(Debug)]
pub enum Event {
  Relay(client::Event),
  Ping(PingEvent),
  Identify(IdentifyEvent),
  Dcutr(dcutr::behaviour::Event),
  Kademlia(KademliaEvent),
}

impl From<client::Event> for Event {
  fn from(e: client::Event) -> Self {
    Event::Relay(e)
  }
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

impl From<dcutr::behaviour::Event> for Event {
  fn from(e: dcutr::behaviour::Event) -> Self {
    Event::Dcutr(e)
  }
}

impl From<KademliaEvent> for Event {
  fn from(e: KademliaEvent) -> Self {
    Event::Kademlia(e)
  }
}
