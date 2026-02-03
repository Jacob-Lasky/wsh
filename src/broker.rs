use bytes::Bytes;
use tokio::sync::broadcast;

pub const BROADCAST_CAPACITY: usize = 64;

#[derive(Clone)]
pub struct Broker {
    tx: broadcast::Sender<Bytes>,
}

impl Broker {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self { tx }
    }

    pub fn publish(&self, data: Bytes) {
        // Ignore error - means no receivers
        let _ = self.tx.send(data);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Bytes> {
        self.tx.subscribe()
    }
}

impl Default for Broker {
    fn default() -> Self {
        Self::new()
    }
}
