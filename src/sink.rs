use std::any::Any;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

use anyhow::Result;
use ringbuf::producer::PostponedProducer;
use ringbuf::{HeapRb, Rb};

use crate::config;
use crate::config::Named;
use crate::device::Device;
use crate::pipe::Pipe;
use crate::switcher::{Control, Port, Switcher};

pub trait SinkStream: Any {}

pub struct Sink {
    pub kind: &'static str,

    muted: Arc<AtomicBool>,
    volume: Arc<AtomicU8>,

    //stream: Box<dyn SinkStream>,
    switcher: Switcher<Sender>,

    sources: HashMap<Arc<String>, Control<Sender>>,
}

pub struct Sender {
    tx: PostponedProducer<i16, Arc<HeapRb<i16>>>,

    muted: Arc<AtomicBool>,
    volume: Arc<AtomicU8>,
}

impl Sender {
    pub fn send(&mut self, data: &[i16]) {
        let muted = self.muted.load(Ordering::Relaxed);
        let volume = self.volume.load(Ordering::Relaxed);

        for &sample in data {
            let sample = if muted {
                0
            } else {
                (sample as i32 * volume as i32 / u8::MAX as i32) as i16
            };

            let _ = self.tx.push(sample);
        }

        self.tx.sync();
    }
}

pub trait SinkType {
    type Config;

    type Stream: SinkStream;

    fn sink(
        name: &str,
        config: Self::Config,
        rx: ringbuf::HeapConsumer<i16>,
    ) -> Result<Self::Stream>;
}

impl Sink {
    pub fn with_config(config: Named<config::Sink>) -> Result<(Named<Self>, Box<dyn SinkStream>)> {
        let (named, config) = config.take();

        let kind = match &config {
            config::Sink::Device(_) => "device",
            config::Sink::Pipe(_) => "pipe",
        };

        let mut ring = HeapRb::<i16>::new(48000 * 2);
        for _ in 0..128 {
            ring.push(0i16).expect("Fill ring buffer");
        }

        let (tx, rx) = ring.split();

        let stream = match config {
            config::Sink::Pipe(config) => {
                Box::new(Pipe::sink(named.name(), config, rx)?) as Box<dyn SinkStream>
            }
            config::Sink::Device(config) => {
                Box::new(Device::sink(named.name(), config, rx)?) as Box<dyn SinkStream>
            }
        };

        let muted = Arc::new(AtomicBool::new(false));
        let volume = Arc::new(AtomicU8::new(u8::MAX));

        let sender = Sender {
            tx: tx.into_postponed(),
            muted: muted.clone(),
            volume: volume.clone(),
        };

        let switcher = Switcher::new(sender);

        return Ok((
            named.with(Sink {
                kind,
                muted,
                volume,
                switcher,
                sources: HashMap::new(),
            }),
            stream,
        ));
    }

    pub fn muted(&self) -> bool {
        return self.muted.load(Ordering::Relaxed);
    }

    pub fn volume(&self) -> u8 {
        return self.volume.load(Ordering::Relaxed);
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }

    pub fn set_volume(&mut self, volume: u8) {
        self.volume.store(volume, Ordering::Relaxed);
    }

    pub fn get_source(&mut self, name: &Arc<String>) -> Option<&Control<Sender>> {
        return self.sources.get(name);
    }

    pub fn get_active_source(&self) -> Option<(Arc<String>, &Control<Sender>)> {
        for (name, source) in self.sources.iter() {
            if source.is_active() {
                return Some((name.clone(), source));
            }
        }

        return None;
    }

    pub fn add_source(&mut self, name: Arc<String>) -> Port<Sender> {
        let (port, control) = self.switcher.port();
        self.sources.insert(name, control);
        return port;
    }
}
