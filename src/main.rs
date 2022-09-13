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
use libp2p::autonat;
use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::upgrade::SelectUpgrade;
use libp2p::identify::{IdentifyEvent, IdentifyInfo};
use libp2p::kad::store::MemoryStore;
use libp2p::kad::{Kademlia, KademliaConfig};
use libp2p::mplex::MplexConfig;
use libp2p::relay::v2::relay::Relay;
use libp2p::yamux::{WindowUpdateMode, YamuxConfig};
use libp2p::{
  core::upgrade,
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
use tokio::time::Instant;

use crate::constants::KEY_SEED;

const BOOTSTRAP_INTERVAL: Duration = Duration::from_secs(3 * 60);

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

  // let (relay_transport, client) = client::Client::new_transport_and_behaviour(local_peer_id);

  let yamux_config = {
    let mut config = YamuxConfig::default();
    config.set_max_buffer_size(16 * 1024 * 1024);
    config.set_receive_window_size(16 * 1024 * 1024);
    config.set_window_update_mode(WindowUpdateMode::on_receive());
    config
  };

  let multiplex_upgrade = SelectUpgrade::new(yamux_config, MplexConfig::new());

  // Create a tokio-based TCP transport use noise for authenticated
  // encryption and Mplex for multiplexing of substreams on a TCP stream.
  let transport = TokioTcpTransport::new(GenTcpConfig::default().nodelay(true))
    .upgrade(upgrade::Version::V1)
    .authenticate(noise::NoiseConfig::xx(noise_keys).into_authenticated())
    .multiplex(multiplex_upgrade)
    .map(|(peer_id, muxer), _| (peer_id, StreamMuxerBox::new(muxer)))
    .map_err(|err| Error::new(ErrorKind::Other, err))
    .boxed();

  let mut swarm = {
    let mut config = KademliaConfig::default();
    config
      .set_query_timeout(Duration::from_secs(60))
      .set_connection_idle_timeout(Duration::from_secs(60));
    let store = MemoryStore::new(local_peer_id);
    let kademlia = Kademlia::with_config(local_peer_id, store, config);

    let autonat = autonat::Behaviour::new(PeerId::from(local_key.public()), Default::default());

    let behaviour = Behaviour {
      relay: Relay::new(PeerId::from(local_key.public()), Default::default()),
      ping: Ping::new(PingConfig::new().with_keep_alive(true)),
      identify: Identify::new(IdentifyConfig::new(
        "/TODO/0.0.1".to_string(),
        local_key.public(),
      )),
      kademlia,
      autonat,
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
      .with(Protocol::Tcp(4003)),
  )?;

  let sleep = tokio::time::sleep(BOOTSTRAP_INTERVAL);
  tokio::pin!(sleep);

  loop {
    select! {
      () = &mut sleep => {
        sleep.as_mut().reset(Instant::now() + BOOTSTRAP_INTERVAL);
        let _ = swarm.behaviour_mut().kademlia.bootstrap();
      }
      event = swarm.select_next_some() => {
        match event {
          SwarmEvent::NewListenAddr { address, .. } => {
              info!("Listening on {:?}", address);
          }
          SwarmEvent::Behaviour(Event::Identify(event)) => {
              info!("{:?}", event);
              if let IdentifyEvent::Received { peer_id, info: IdentifyInfo { listen_addrs, protocols, .. } } = event {
                if protocols
                  .iter()
                  .any(|p| p.as_bytes() == libp2p::kad::protocol::DEFAULT_PROTO_NAME)
                {
                  for addr in listen_addrs {
                    swarm
                      .behaviour_mut()
                      .kademlia
                      .add_address(&peer_id, addr);
                  }
                }
              };
          }
          SwarmEvent::Behaviour(Event::Ping(_)) => {}
          SwarmEvent::Behaviour(Event::Autonat(e)) => {
            info!("{e:?}");
          }
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
