use std::time::Instant;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BootPhase {
    Critical,
    Deferred,
}

pub struct BootProfile {
    records: Mutex<HashMap<&'static str, (BootPhase, std::time::Duration)>>,
}

impl BootProfile {
    pub fn new() -> Self {
        Self {
            records: Mutex::new(HashMap::new()),
        }
    }

    pub fn record<T, F: FnOnce() -> T>(&self, name: &'static str, phase: BootPhase, f: F) -> T {
        let start = Instant::now();
        let res = f();
        let elapsed = start.elapsed();
        if let Ok(mut lock) = self.records.lock() {
            lock.insert(name, (phase, elapsed));
        }
        res
    }

    pub async fn record_async<T, F: std::future::Future<Output = T>>(&self, name: &'static str, phase: BootPhase, f: F) -> T {
        let start = Instant::now();
        let res = f.await;
        let elapsed = start.elapsed();
        if let Ok(mut lock) = self.records.lock() {
            lock.insert(name, (phase, elapsed));
        }
        res
    }

    pub fn print_if_requested(&self) {
        if std::env::var("VAC_BOOT_PROFILE").unwrap_or_default() == "1" {
            if let Ok(lock) = self.records.lock() {
                println!("{:<30} | {:<10} | {:<10}", "Step", "Phase", "Duration");
                println!("{:-<30}-+-{:-<10}-+-{:-<10}", "", "", "");
                let mut entries: Vec<_> = lock.iter().collect();
                entries.sort_by_key(|(_, (_, d))| *d);
                for (name, (phase, duration)) in entries.iter().rev() {
                    println!("{:<30} | {:<10?} | {:?}", name, phase, duration);
                }
            }
        }
    }
}

pub fn boot_profile() -> &'static BootProfile {
    static INSTANCE: OnceLock<BootProfile> = OnceLock::new();
    INSTANCE.get_or_init(|| BootProfile::new())
}
