use futures::Future;
use libp2p::identity;
use tokio::{runtime::Handle, task};

pub fn block_on<F: Future>(f: F) -> F::Output {
  task::block_in_place(|| Handle::current().block_on(f))
}

pub fn generate_ed25519(secret_key_seed: u8) -> identity::Keypair {
  let mut bytes = [0u8; 32];
  bytes[0] = secret_key_seed;

  let secret_key = identity::ed25519::SecretKey::from_bytes(&mut bytes)
    .expect("this returns `Err` only if the length is wrong; the length is correct; qed");
  identity::Keypair::Ed25519(secret_key.into())
}
