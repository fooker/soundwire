#![feature(trait_upcasting)]

use std::any::Any;
use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use structopt::StructOpt;
use tracing::{info, Level};

use crate::config::Config;
use crate::proto::State;
use crate::sink::{Sender, Sink};
use crate::source::{Source, SourceCallback};
use crate::switcher::Port;

mod config;
mod sink;
mod source;

mod switcher;

mod proto;

mod device;
mod pipe;

#[derive(StructOpt, Debug)]
#[structopt(name = "soundwire", about = "audio routing daemon")]
pub struct Opt {
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    verbose: usize,

    #[structopt(short = "c", long = "config", default_value = "soundwire.conf")]
    config: PathBuf,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let opt = Opt::from_args();

    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(match opt.verbose {
            0 => Level::WARN,
            1 => Level::INFO,
            2 => Level::DEBUG,
            _ => Level::TRACE,
        })
        .init();

    let config = Config::load(&opt.config)
        .with_context(|| format!("Failed to load config: {}", opt.config.display()))?;

    info!("Welcome to SoundWire!");

    let mut sinks = HashMap::new();
    let mut sources = HashMap::new();

    let mut workers = Vec::<Box<dyn Any>>::new();

    for config in config.outputs {
        let (sink, worker) = Sink::with_config(config)?;
        info!("Created sink: {}", sink.name);

        sinks.insert(sink.name.clone(), sink);
        workers.push(worker);
    }

    for config in config.sources {
        let mut ports = Vec::new();

        for (_, sink) in &mut sinks {
            let port = sink.add_source(config.name.clone());
            ports.push(port);
        }

        let broadcaster = Broadcaster { ports };

        let (source, worker) = Source::with_config(config, broadcaster)?;
        info!("Created source: {}", source.name);

        sources.insert(source.name.clone(), source);
        workers.push(worker);
    }

    info!("Initialisation completed");

    proto::serve(State { sinks, sources }).await?;

    // TODO: Really join threads here
    for worker in workers {
        drop(worker);
    }

    return Ok(());
}

pub struct Broadcaster {
    ports: Vec<Port<Sender>>,
}

impl SourceCallback for Broadcaster {
    fn data(&mut self, data: &[i16]) {
        for port in self.ports.iter() {
            if let Some(port) = &mut *port.access() {
                port.send(data);
            }
        }
    }

    fn idle(&mut self) {}
}
