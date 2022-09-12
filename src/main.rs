mod behaviour;
mod constants;
mod event;
mod helper;
mod opts;

use std::net::Ipv4Addr;
use std::str::FromStr;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use futures::executor::block_on;
use futures::stream::StreamExt;
use libp2p::core::transport::OrTransport;
use libp2p::core::upgrade::SelectUpgrade;
use libp2p::dns::DnsConfig;
use libp2p::gossipsub::{self, MessageAuthenticity, ValidationMode};
use libp2p::identify::{IdentifyEvent, IdentifyInfo};
use libp2p::kad::store::MemoryStore;
use libp2p::kad::{Kademlia, KademliaConfig};
use libp2p::mdns::{Mdns, MdnsConfig, MdnsEvent};
use libp2p::mplex::MplexConfig;
use libp2p::relay::v2::client::{self, Client};
use libp2p::yamux::{WindowUpdateMode, YamuxConfig};
use libp2p::{
  core::upgrade,
  dcutr,
  gossipsub::GossipsubEvent,
  identify::{Identify, IdentifyConfig},
  multiaddr::Protocol,
  noise,
  ping::{Ping, PingConfig},
  swarm::{SwarmBuilder, SwarmEvent},
  tcp::{GenTcpConfig, TokioTcpTransport},
  Multiaddr, PeerId, Transport,
};
use log::{error, info};
use rand::Rng;
use tokio::io::AsyncBufReadExt;

use behaviour::Behaviour;
use event::Event;
use helper::generate_ed25519;
use opts::{Mode, Opts};
use tokio::select;

use crate::constants::{BOODSTRAP_ADDRESS, BOOT_NODES};

#[tokio::main]
async fn main() -> Result<()> {
  env_logger::init();

  let opts = Opts::parse();
  info!("{opts:?}");

  let mut rng = rand::thread_rng();
  let random_seed: u8 = rng.gen();
  let local_key = generate_ed25519(random_seed);
  let local_peer_id = PeerId::from(local_key.public());
  info!("Local peer id: {:?}", local_peer_id);

  let (relay_transport, client) = Client::new_transport_and_behaviour(local_peer_id);

  // Create a keypair for authenticated encryption of the transport.
  let noise_keys = noise::Keypair::<noise::X25519Spec>::new()
    .into_authentic(&local_key)
    .expect("Signing libp2p-noise static DH keypair failed.");

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
    block_on(DnsConfig::system(TokioTcpTransport::new(
      GenTcpConfig::default().port_reuse(true),
    )))
    .unwrap(),
  )
  .upgrade(upgrade::Version::V1)
  .authenticate(noise::NoiseConfig::xx(noise_keys).into_authenticated())
  .multiplex(multiplex_upgrade)
  .boxed();

  // Create a Gossipsub topic
  let topic = gossipsub::IdentTopic::new("chat");

  let mut swarm = {
    // Set mDNS
    let mdns = block_on(Mdns::new(MdnsConfig::default()))?;

    // Set a custom gossipsub
    let gossipsub_config = gossipsub::GossipsubConfigBuilder::default()
      .heartbeat_interval(std::time::Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
      .validation_mode(ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
      .build()
      .expect("Valid config");

    // Build a gossipsub network behaviour
    let mut gossipsub: gossipsub::Gossipsub = gossipsub::Gossipsub::new(
      MessageAuthenticity::Signed(local_key.clone()),
      gossipsub_config,
    )
    .expect("Correct configuration");

    // Subscribes to our topic
    gossipsub.subscribe(&topic).unwrap();

    let mut config = KademliaConfig::default();
    config.set_query_timeout(Duration::from_secs(5 * 60));
    let store = MemoryStore::new(local_peer_id);
    let kademlia = Kademlia::with_config(local_peer_id, store, config);

    let mut behaviour = Behaviour {
      client,
      ping: Ping::new(PingConfig::new()),
      identify: Identify::new(IdentifyConfig::new(
        "/TODO/0.0.1".to_string(),
        local_key.public(),
      )),
      dcutr: dcutr::behaviour::Behaviour::new(),
      gossipsub,
      mdns,
      kademlia,
    };

    for peer in BOOT_NODES {
      behaviour.kademlia.add_address(
        &PeerId::from_str(peer)?,
        BOODSTRAP_ADDRESS.parse::<Multiaddr>()?,
      );
    }

    behaviour.kademlia.bootstrap()?;

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

  block_on(async {
    loop {
      select! {
          event = swarm.next() => {
              match event.unwrap() {
                  SwarmEvent::NewListenAddr { address, .. } => {
                      println!("NewListenAddr Listening on {:?}", address);
                  }
                  event => {
                    info!("{:?}", event)
                  },
              }
          }
          _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
              // Likely listening on all interfaces now, thus continuing by breaking the loop.
              break;
          }
      }
    }
  });

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
          println!("Told relay its public address.");
          told_relay_observed_addr = true;
        }
        SwarmEvent::Behaviour(Event::Identify(IdentifyEvent::Received {
          info: IdentifyInfo { observed_addr, .. },
          ..
        })) => {
          println!("Relay told us our public address: {:?}", observed_addr);
          learned_observed_addr = true;
        }
        // SwarmEvent::Behaviour(Event::Mdns(event)) => match event {
        //   MdnsEvent::Discovered(list) => {
        //     for (peer, _) in list {
        //       swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer);
        //     }
        //   }
        //   MdnsEvent::Expired(list) => {
        //     for (peer, _) in list {
        //       if !swarm.behaviour().mdns.has_node(&peer) {
        //         swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer);
        //       }
        //     }
        //   }
        // },
        event => info!("{event:?}"),
      }

      if learned_observed_addr && told_relay_observed_addr {
        break;
      }
    }
  });

  let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();

  loop {
    tokio::select! {
      line = stdin.next_line() => {
        let line = line?.expect("stdin closed");
        if let Err(e) = swarm
          .behaviour_mut()
          .gossipsub
          .publish(topic.clone(), line.as_bytes()) {
            info!("{e:?}");
        }
      }
      event = swarm.select_next_some() => {
        match event {
          SwarmEvent::Behaviour(Event::Gossipsub(GossipsubEvent::Message { message, .. })) => {
            info!(
              "Received: '{:?}' from {:?}",
              String::from_utf8_lossy(&message.data),
              message.source
            );
          }
          SwarmEvent::NewListenAddr { address, .. } => {
              info!("Listening on {:?}", address);
          }
          SwarmEvent::Behaviour(Event::Mdns(event)) => {
            info!("{event:?}");
            match event {
              MdnsEvent::Discovered(list) => {
                for (peer, _) in list {
                  swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer);
                }
              }
              MdnsEvent::Expired(list) => {
                for (peer, _) in list {
                  if !swarm.behaviour().mdns.has_node(&peer) {
                    swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer);
                  }
                }
              }
            }
          },
          SwarmEvent::Behaviour(Event::Client(client::Event::ReservationReqAccepted {
              ..
          })) => {
              assert!(opts.mode == Mode::Listen);
              info!("Relay accepted our reservation request.");
          }
          SwarmEvent::Behaviour(Event::Client(event)) => {
              info!("{:?}", event)
          }
          SwarmEvent::Behaviour(Event::Dcutr(event)) => {
              info!("{:?}", event)
          }
          SwarmEvent::Behaviour(Event::Identify(event)) => {
              info!("{:?}", event);
              if let IdentifyEvent::Received {
                peer_id,
                info:
                  IdentifyInfo {
                    listen_addrs,
                    protocols,
                    ..
                  },
              } = event
              {
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
              }
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
