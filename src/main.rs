mod behaviour;
mod constants;
mod event;
mod helper;
mod opts;

use std::io::{Error, ErrorKind};
use std::net::Ipv4Addr;
use std::time::Duration;

use anyhow::Result;
use futures::StreamExt;
use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::transport::OrTransport;
use libp2p::core::upgrade::SelectUpgrade;
use libp2p::dns::TokioDnsConfig;
use libp2p::identify::{IdentifyEvent, IdentifyInfo};
use libp2p::kad::store::MemoryStore;
use libp2p::kad::{Kademlia, KademliaConfig};
use libp2p::mplex::MplexConfig;
use libp2p::relay::v2::client;
use libp2p::yamux::{WindowUpdateMode, YamuxConfig};
use libp2p::{
  core::upgrade,
  dcutr,
  identify::{Identify, IdentifyConfig},
  multiaddr::Protocol,
  noise,
  ping::{Ping, PingConfig},
  swarm::{SwarmBuilder, SwarmEvent},
  tcp::{GenTcpConfig, TokioTcpTransport},
  Multiaddr, PeerId, Transport,
};
use log::{error, info};

use behaviour::Behaviour;
use event::Event;
use helper::generate_ed25519;
use tokio::select;

use crate::constants::{KEY_SEED, RELAY_NODE};
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

  let (relay_transport, client) = client::Client::new_transport_and_behaviour(local_peer_id);

  let yamux_config = {
    let mut config = YamuxConfig::default();
    config.set_max_buffer_size(16 * 1024 * 1024);
    config.set_receive_window_size(16 * 1024 * 1024);
    config.set_window_update_mode(WindowUpdateMode::on_receive());
    config
  };

  let multiplex_upgrade = SelectUpgrade::new(yamux_config, MplexConfig::new());

  let transport = OrTransport::new(
    relay_transport,
    TokioDnsConfig::system(TokioTcpTransport::new(
      GenTcpConfig::default().nodelay(true).port_reuse(true),
    ))?,
  )
  .upgrade(upgrade::Version::V1)
  .authenticate(noise::NoiseConfig::xx(noise_keys).into_authenticated())
  .multiplex(multiplex_upgrade)
  .map(|(peer_id, muxer), _| (peer_id, StreamMuxerBox::new(muxer)))
  .map_err(|err| Error::new(ErrorKind::Other, err))
  .boxed();

  let mut swarm = {
    let mut config = KademliaConfig::default();
    config.set_query_timeout(Duration::from_secs(5 * 60));
    let store = MemoryStore::new(local_peer_id);
    let kademlia = Kademlia::with_config(local_peer_id, store, config);

    let behaviour = Behaviour {
      relay: client,
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

  swarm.listen_on(
    Multiaddr::empty()
      .with("0.0.0.0".parse::<Ipv4Addr>().unwrap().into())
      .with(Protocol::Tcp(0)),
  )?;

  block_on(async {
    loop {
      select! {
          event = swarm.next() => {
              match event.unwrap() {
                  SwarmEvent::NewListenAddr { address, .. } => {
                      info!("Listening on {:?}", address);
                      // swarm.behaviour_mut().kademlia.add_address(&local_peer_id, address);
                  }
                  event => panic!("{:?}", event),
              }
          }
          _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {
              // Likely listening on all interfaces now, thus continuing by breaking the loop.
              break;
          }
      }
    }
  });

  swarm.dial(RELAY_NODE.parse::<Multiaddr>()?)?;
  block_on(async {
    let mut learned_observed_addr = false;
    let mut told_relay_observed_addr = false;

    loop {
      match swarm.next().await.unwrap() {
        SwarmEvent::NewListenAddr { .. } => {}
        SwarmEvent::Dialing { .. } => {}
        SwarmEvent::ConnectionEstablished { .. } => {}
        SwarmEvent::Behaviour(Event::Ping(_)) => {}
        SwarmEvent::Behaviour(Event::Identify(IdentifyEvent::Sent { .. })) => {
          info!("Told relay its public address.");
          told_relay_observed_addr = true;
        }
        SwarmEvent::Behaviour(Event::Identify(IdentifyEvent::Received {
          info: IdentifyInfo { observed_addr, .. },
          ..
        })) => {
          info!("Relay told us our public address: {:?}", observed_addr);
          learned_observed_addr = true;
        }
        event => panic!("{:?}", event),
      }

      if learned_observed_addr && told_relay_observed_addr {
        break;
      }
    }
  });

  swarm.listen_on(RELAY_NODE.parse::<Multiaddr>()?.with(Protocol::P2pCircuit))?;

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
