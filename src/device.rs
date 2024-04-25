use std::time::Duration;

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};
use lazy_static::lazy_static;
use ringbuf::HeapConsumer;
use tracing::error;

use crate::config;
use crate::sink::{SinkStream, SinkType};
use crate::source::{SourceCallback, SourceStream, SourceType};

lazy_static! {
    static ref HOST: cpal::Host = cpal::default_host();
}

pub struct Device;

impl SourceStream for Stream {}

impl SinkStream for Stream {}

impl SourceType for Device {
    type Config = config::DeviceSource;

    type Stream = Stream;

    fn source(
        _name: &str,
        _config: Self::Config,
        mut callback: impl SourceCallback + 'static,
    ) -> Result<Self::Stream> {
        let device = HOST
            .default_input_device() // TODO: search for configured device
            .context("No default input device")?;

        let config: StreamConfig = device.default_input_config()?.into();

        let stream = device.build_input_stream(
            &config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                callback.data(data);
            },
            |err: cpal::StreamError| {
                error!("Device input stream error: {}", err);
            },
            Some(Duration::from_millis(100)),
        )?;

        stream.play()?;

        return Ok(stream);
    }
}

impl SinkType for Device {
    type Config = config::DeviceSink;
    type Stream = Stream;

    fn sink(_name: &str, _config: Self::Config, mut rx: HeapConsumer<i16>) -> Result<Self::Stream> {
        let device = HOST
            .default_output_device() // TODO: search for configured device
            .context("No default output device")?;

        let config: StreamConfig = device.default_output_config()?.into();

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                let r = rx.pop_slice(data);
                if r < data.len() {
                    data[r..].fill(0i16);
                    // eprintln!("Output underflow");
                }
            },
            |err: cpal::StreamError| {
                error!("Device output stream error: {}", err);
            },
            Some(Duration::from_millis(100)),
        )?;

        stream.play()?;

        return Ok(stream);
    }
}
