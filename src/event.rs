use libp2p::identify::IdentifyEvent;
use libp2p::kad::KademliaEvent;
use libp2p::ping::PingEvent;
use libp2p::relay::v2::relay;

#[derive(Debug)]
pub enum Event {
  Relay(relay::Event),
  Ping(PingEvent),
  Identify(IdentifyEvent),
  Kademlia(KademliaEvent),
}

impl From<relay::Event> for Event {
  fn from(e: relay::Event) -> Self {
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

impl From<KademliaEvent> for Event {
  fn from(e: KademliaEvent) -> Self {
    Event::Kademlia(e)
  }
}
