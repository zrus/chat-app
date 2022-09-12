use libp2p::dcutr;
use libp2p::identify::IdentifyEvent;
use libp2p::kad::KademliaEvent;
use libp2p::ping::PingEvent;

#[derive(Debug)]
pub enum Event {
  Ping(PingEvent),
  Identify(IdentifyEvent),
  Dcutr(dcutr::behaviour::Event),
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
