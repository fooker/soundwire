use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use url::Url;

use crate::config;
use crate::config::Named;
use crate::device::Device;
use crate::pipe::Pipe;

pub trait SourceStream: Any {}

pub struct Source {
    #[allow(unused)]
    kind: &'static str,

    active: Arc<AtomicBool>,
}

pub trait SourceCallback: Send {
    fn data(&mut self, data: &[i16]);
    fn idle(&mut self);
}

pub trait SourceType {
    type Config;

    type Stream: SourceStream;

    fn source(
        name: &str,
        config: Self::Config,
        callback: impl SourceCallback + 'static,
    ) -> Result<Self::Stream>;
}

impl Source {
    pub fn with_config(
        config: Named<config::Source>,
        callback: impl SourceCallback + 'static,
    ) -> Result<(Named<Self>, Box<dyn SourceStream>)> {
        let (named, config) = config.take();

        let kind = match &config {
            config::Source::Pipe(_) => "pipe",
            config::Source::Device(_) => "device",
        };

        let active = Arc::new(AtomicBool::new(false));

        let callback = MonitoringSourceCallback {
            inner: callback,
            active: active.clone(),
        };

        let stream = match config {
            config::Source::Pipe(config) => {
                Box::new(Pipe::source(named.name(), config, callback)?) as Box<dyn SourceStream>
            }
            config::Source::Device(config) => {
                Box::new(Device::source(named.name(), config, callback)?) as Box<dyn SourceStream>
            }
        };

        return Ok((named.with(Self { kind, active }), stream));
    }

    pub fn uri(&self) -> Url {
        return Url::parse(&format!("{}://", self.kind)).expect("valid url"); // TODO: make this a real URI including parameters
    }

    pub fn is_active(&self) -> bool {
        return self.active.load(Ordering::Relaxed);
    }
}

struct MonitoringSourceCallback<C: SourceCallback> {
    inner: C,
    active: Arc<AtomicBool>,
}

impl<C: SourceCallback> Drop for MonitoringSourceCallback<C> {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Relaxed);
    }
}

impl<C: SourceCallback> SourceCallback for MonitoringSourceCallback<C> {
    fn data(&mut self, data: &[i16]) {
        self.active.store(true, Ordering::Relaxed);
        self.inner.data(data);
    }

    fn idle(&mut self) {
        self.active.store(false, Ordering::Relaxed);
        self.inner.idle();
    }
}
