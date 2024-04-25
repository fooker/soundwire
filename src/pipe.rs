use anyhow::Result;
use byteorder::{NativeEndian, ReadBytesExt, WriteBytesExt};
use std::fs::File;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use crate::config;
use crate::sink::{SinkStream, SinkType};
use crate::source::{SourceCallback, SourceStream, SourceType};

pub struct Pipe;

pub struct PipeSourceStream {
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<()>>>,
}

pub struct PipeSinkStream {
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<()>>>,
}

impl SourceStream for PipeSourceStream {}

impl Drop for PipeSourceStream {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap().unwrap(); // TODO: Error handling
    }
}

impl SinkStream for PipeSinkStream {}

impl Drop for PipeSinkStream {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap().unwrap(); // TODO: Error handling
    }
}

impl SourceType for Pipe {
    type Config = config::PipeSource;
    type Stream = PipeSourceStream;

    fn source(
        _name: &str,
        config: Self::Config,
        callback: impl SourceCallback + 'static,
    ) -> Result<Self::Stream> {
        if let Some(path) = config.path.parent() {
            std::fs::create_dir_all(path)?;
        }

        if config.create {
            nix::unistd::mkfifo(&config.path, nix::sys::stat::Mode::all())?;
        }

        let running = Arc::new(AtomicBool::new(true));

        let f = std::fs::OpenOptions::new().read(true).open(&config.path)?;

        let thread = std::thread::spawn(source_worker(callback, f, running.clone()));

        return Ok(Self::Stream {
            running,
            thread: Some(thread),
        });
    }
}

fn source_worker(
    mut callback: impl SourceCallback,
    mut f: File,
    running: Arc<AtomicBool>,
) -> impl FnOnce() -> Result<()> {
    return move || {
        let mut data = [0i16; 64];

        while running.load(Ordering::Relaxed) {
            f.read_i16_into::<NativeEndian>(&mut data)?;

            callback.data(&data);
        }

        return Ok(());
    };
}

impl SinkType for Pipe {
    type Config = config::PipeSink;
    type Stream = PipeSinkStream;

    fn sink(
        _name: &str,
        config: Self::Config,
        rx: ringbuf::HeapConsumer<i16>,
    ) -> Result<Self::Stream> {
        if let Some(path) = config.path.parent() {
            std::fs::create_dir_all(path)?;
        }

        if config.create {
            nix::unistd::mkfifo(&config.path, nix::sys::stat::Mode::all())?;
        }

        let running = Arc::new(AtomicBool::new(true));

        let f = std::fs::OpenOptions::new().write(true).open(&config.path)?;

        let thread = std::thread::spawn(sink_worker(rx, f, running.clone()));

        return Ok(Self::Stream {
            running,
            thread: Some(thread),
        });
    }
}

fn sink_worker(
    mut rx: ringbuf::HeapConsumer<i16>,
    mut f: File,
    running: Arc<AtomicBool>,
) -> impl FnOnce() -> Result<()> {
    return move || {
        let mut data = [0i16; 64];

        while running.load(Ordering::Relaxed) {
            let i = rx.pop_slice(&mut data);

            for d in &data[0..i] {
                f.write_i16::<NativeEndian>(*d)?;
            }
        }

        return Ok(());
    };
}
