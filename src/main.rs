mod behaviour;
mod constants;
mod event;
mod helper;
mod opts;

use std::net::Ipv4Addr;
use std::time::Duration;

use anyhow::Result;
use futures::StreamExt;
use libp2p::kad::store::MemoryStore;
use libp2p::kad::{Kademlia, KademliaConfig};
use libp2p::relay::v2::client;
use libp2p::{
  core::{transport::OrTransport, upgrade},
  dcutr,
  dns::DnsConfig,
  identify::{Identify, IdentifyConfig},
  multiaddr::Protocol,
  noise,
  ping::{Ping, PingConfig},
  relay::v2::client::Client,
  swarm::{SwarmBuilder, SwarmEvent},
  tcp::{GenTcpConfig, TokioTcpTransport},
  Multiaddr, PeerId, Transport,
};
use log::{error, info};

use behaviour::Behaviour;
use event::Event;
use helper::generate_ed25519;
use tokio::select;

use crate::constants::KEY_SEED;
use crate::helper::block_on;

#[tokio::main]
async fn main() -> Result<()> {
  env_logger::init();

  let local_key = generate_ed25519(KEY_SEED);
  let local_peer_id = PeerId::from(local_key.public());
  info!("Local peer id: {:?}", local_peer_id);

  // Create a keypair for authenticated encryption of the transport.
  let noise_keys = noise::Keypair::<noise::X25519Spec>::new()
    .into_authentic(&local_key)
    .expect("Signing libp2p-noise static DH keypair failed.");

  let (relay_transport, client) = Client::new_transport_and_behaviour(local_peer_id);

  // Create a tokio-based TCP transport use noise for authenticated
  // encryption and Mplex for multiplexing of substreams on a TCP stream.
  let transport = OrTransport::new(
    relay_transport,
    block_on(DnsConfig::system(TokioTcpTransport::new(
      GenTcpConfig::default().nodelay(true).port_reuse(true),
    )))
    .unwrap(),
  )
  .upgrade(upgrade::Version::V1)
  .authenticate(noise::NoiseConfig::xx(noise_keys).into_authenticated())
  .multiplex(libp2p_yamux::YamuxConfig::default())
  .boxed();

  let mut swarm = {
    let mut config = KademliaConfig::default();
    config.set_query_timeout(Duration::from_secs(5 * 60));
    let store = MemoryStore::new(local_peer_id);
    let kademlia = Kademlia::with_config(local_peer_id, store, config);

    let behaviour = Behaviour {
      relay_client: client,
      ping: Ping::new(PingConfig::new()),
      identify: Identify::new(IdentifyConfig::new(
        "/TODO/0.0.1".to_string(),
        local_key.public(),
      )),
      dcutr: dcutr::behaviour::Behaviour::new(),
      kademlia,
    };

    // build the swarm
    SwarmBuilder::new(transport, behaviour, local_peer_id)
      .executor(Box::new(|fut| {
        tokio::spawn(fut);
      }))
      .build()
  };

  swarm
    .listen_on(
      Multiaddr::empty()
        .with("0.0.0.0".parse::<Ipv4Addr>().unwrap().into())
        .with(Protocol::Tcp(0)),
    )
    .unwrap();

  loop {
    select! {
      event = swarm.select_next_some() => {
        match event {
          SwarmEvent::NewListenAddr { address, .. } => {
              info!("Listening on {:?}", address);
          }
          SwarmEvent::Behaviour(Event::Relay(client::Event::ReservationReqAccepted {
              ..
          })) => {
              info!("Relay accepted our reservation request.");
          }
          SwarmEvent::Behaviour(Event::Relay(event)) => {
              info!("{:?}", event)
          }
          SwarmEvent::Behaviour(Event::Dcutr(event)) => {
              info!("{:?}", event)
          }
          SwarmEvent::Behaviour(Event::Identify(event)) => {
              info!("{:?}", event)
          }
          SwarmEvent::Behaviour(Event::Ping(_)) => {}
          SwarmEvent::ConnectionEstablished {
              peer_id, endpoint, ..
          } => {
              info!("Established connection to {:?} via {:?}", peer_id, endpoint);
          }
          SwarmEvent::OutgoingConnectionError { peer_id, error } => {
              error!("Outgoing connection error to {:?}: {:?}", peer_id, error);
          }
          event => info!("Other: {event:?}"),
        }
      }
    }
  }
}
