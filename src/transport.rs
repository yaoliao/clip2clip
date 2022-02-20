use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use anyhow::Result;
use arboard::{Clipboard, ImageData};
use futures::StreamExt;
use libp2p::{gossipsub, identity, Multiaddr, PeerId, swarm::SwarmEvent};
use libp2p::gossipsub::{Gossipsub, MessageId};
use libp2p::gossipsub::{
    GossipsubEvent, GossipsubMessage, IdentTopic as Topic, MessageAuthenticity, ValidationMode,
};
use libp2p::mdns::{Mdns, MdnsEvent};
use libp2p::NetworkBehaviour;
use libp2p::swarm::NetworkBehaviourEventProcess;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncBufReadExt;


#[derive(Debug, Serialize, Deserialize)]
pub struct Msg {
    pub mag_type: MsgType,
    pub data: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MsgType {
    TXT,
    IMG { width: usize, height: usize },
}

pub struct ClipHandler {
    receiver: tokio::sync::mpsc::Receiver<Msg>,
}

impl ClipHandler {
    pub fn new(
        receiver: tokio::sync::mpsc::Receiver<Msg>,
    ) -> Self {
        ClipHandler { receiver }
    }

    #[tokio::main]
    pub async fn start_gossip(&mut self) -> Result<()> {
        // Create a random PeerId
        let local_key = identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());
        println!("Local peer id: {:?}", local_peer_id);

        // Set up an encrypted TCP Transport over the Mplex and Yamux protocols
        let transport = libp2p::development_transport(local_key.clone()).await?;

        // Create a Gossipsub topic
        let topic = Topic::new("clip2clip");

        // Create a Swarm to manage peers and events
        let mut swarm = {
            // To content-address message, we can take the hash of message and use it as an ID.
            let message_id_fn = |message: &GossipsubMessage| {
                let mut s = DefaultHasher::new();
                message.data.hash(&mut s);
                MessageId::from(s.finish().to_string())
            };

            // Set a custom gossipsub
            let gossipsub_config = gossipsub::GossipsubConfigBuilder::default()
                .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
                .validation_mode(ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
                .message_id_fn(message_id_fn) // content-address messages. No two messages of the
                // same content will be propagated.
                .build()
                .expect("Valid config");
            // build a gossipsub network behaviour
            let mut gossipsub: gossipsub::Gossipsub =
                gossipsub::Gossipsub::new(MessageAuthenticity::Signed(local_key), gossipsub_config)
                    .expect("Correct configuration");

            // subscribes to our topic
            gossipsub.subscribe(&topic).unwrap();

            // add an explicit peer if one was provided
            if let Some(explicit) = std::env::args().nth(2) {
                let explicit = explicit.clone();
                match explicit.parse() {
                    Ok(id) => gossipsub.add_explicit_peer(&id),
                    Err(err) => println!("Failed to parse explicit peer id: {:?}", err),
                }
            }

            let mdns = Mdns::new(Default::default()).await?;
            let my_behaviour = MyBehaviour { gossipsub, mdns };

            // build the swarm
            libp2p::Swarm::new(transport, my_behaviour, local_peer_id)
        };

        // Listen on all interfaces and whatever port the OS assigns
        swarm
            .listen_on("/ip4/0.0.0.0/tcp/0".parse().unwrap())
            .unwrap();

        // Kick it off
        loop {
            tokio::select! {
                    msg = self.receiver.recv() => match msg {
                        Some(data) => {
                            debug!("receive msg from clipboard :{:?}", data);
                            let bincode = bincode::serialize(&data).expect("serialize msg err");
                            if let Err(e) = swarm.behaviour_mut().gossipsub.publish(topic.clone(), bincode){
                                error!("Publish error: {:?}", e);
                            }
                        }
                        None => { error!("channel closed !!!") }
                    },
                event = swarm.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!("Listening on {:?}", address);
                    }
                    _ => {}
                }
            }
        }
    }
}

pub fn copy_2_clip(msg: Msg) -> Result<()> {
    let mut clip = Clipboard::new()?;
    match msg.mag_type {
        MsgType::TXT => {
            clip.set_text(String::from_utf8_lossy(&msg.data).to_string())?;
            info!("copy txt to clipboard ");
        }
        MsgType::IMG { width: w, height: h } => {
            let img = ImageData {
                width: w,
                height: h,
                bytes: Cow::from(msg.data),
            };
            clip.set_image(img)?;
            info!("copy img to clipboard ");
        }
    }
    Ok(())
}

#[derive(NetworkBehaviour)]
#[behaviour(event_process = true)]
struct MyBehaviour {
    gossipsub: Gossipsub,
    mdns: Mdns,
}

impl NetworkBehaviourEventProcess<GossipsubEvent> for MyBehaviour {
    // Called when `GossipsubEvent` produces an event.
    fn inject_event(&mut self, message: GossipsubEvent) {
        if let GossipsubEvent::Message {
            propagation_source,
            message_id,
            message,
        } = message
        {
            debug!(
                "Received: '{:?}' from {:?}",
                String::from_utf8_lossy(&message.data),
                message.source
            );

            if let Ok(msg) = bincode::deserialize::<Msg>(&message.data) {
                if let Err(e) = copy_2_clip(msg) {
                    error!("copy to clipboard error : {:?}", e);
                }
            }
        }
    }
}

impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour {
    // Called when `mdns` produces an event.
    fn inject_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(list) => {
                for (peer, _) in list {
                    self.gossipsub.add_explicit_peer(&peer);
                }
            }
            MdnsEvent::Expired(list) => {
                for (peer, _) in list {
                    if !self.mdns.has_node(&peer) {
                        self.gossipsub.remove_explicit_peer(&peer);
                    }
                }
            }
        }
    }
}
