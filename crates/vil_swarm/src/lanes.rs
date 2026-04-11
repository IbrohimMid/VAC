//! Tri-Lane communication channels.

use crate::protocol::{ControlPayload, DataPayload, TriggerPayload};
use tokio::sync::mpsc;

pub struct TriggerLane {
    tx: mpsc::Sender<TriggerPayload>,
    rx: mpsc::Receiver<TriggerPayload>,
}

pub struct DataLane {
    tx: mpsc::Sender<DataPayload>,
    rx: mpsc::Receiver<DataPayload>,
}

pub struct ControlLane {
    tx: mpsc::Sender<ControlPayload>,
    rx: mpsc::Receiver<ControlPayload>,
}

impl TriggerLane {
    pub fn new(buffer: usize) -> Self {
        let (tx, rx) = mpsc::channel(buffer);
        Self { tx, rx }
    }
    pub fn sender(&self) -> mpsc::Sender<TriggerPayload> {
        self.tx.clone()
    }
    pub async fn recv(&mut self) -> Option<TriggerPayload> {
        self.rx.recv().await
    }
}

impl DataLane {
    pub fn new(buffer: usize) -> Self {
        let (tx, rx) = mpsc::channel(buffer);
        Self { tx, rx }
    }
    pub fn sender(&self) -> mpsc::Sender<DataPayload> {
        self.tx.clone()
    }
    pub async fn recv(&mut self) -> Option<DataPayload> {
        self.rx.recv().await
    }
}

impl ControlLane {
    pub fn new(buffer: usize) -> Self {
        let (tx, rx) = mpsc::channel(buffer);
        Self { tx, rx }
    }
    pub fn sender(&self) -> mpsc::Sender<ControlPayload> {
        self.tx.clone()
    }
    pub async fn recv(&mut self) -> Option<ControlPayload> {
        self.rx.recv().await
    }
}
