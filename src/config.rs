use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Named<T> {
    pub name: Arc<String>,

    #[serde(flatten)]
    pub value: T,
}

impl<T> Deref for Named<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        return &self.value;
    }
}

impl<T> DerefMut for Named<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        return &mut self.value;
    }
}

impl<T> Named<T> {
    pub fn name(&self) -> &str {
        return &*self.name;
    }

    pub fn take(self) -> (Named<()>, T) {
        return (
            Named {
                name: self.name,
                value: (),
            },
            self.value,
        );
    }

    pub fn with<V>(&self, value: V) -> Named<V> {
        return Named {
            name: self.name.clone(),
            value,
        };
    }
}

#[derive(Deserialize, Debug)]
pub struct PipeSink {
    pub path: PathBuf,

    #[serde(default)]
    pub create: bool,
}

#[derive(Deserialize, Debug)]
pub struct DeviceSink {
    pub device: String,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum Sink {
    Pipe(PipeSink),
    Device(DeviceSink),
}

#[derive(Deserialize, Debug)]
pub struct PipeSource {
    pub path: PathBuf,

    #[serde(default)]
    pub create: bool,
}

#[derive(Deserialize, Debug)]
pub struct DeviceSource {
    pub device: String,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum Source {
    Pipe(PipeSource),
    Device(DeviceSource),
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub outputs: Vec<Named<Sink>>,
    pub sources: Vec<Named<Source>>,
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let mut f = std::fs::OpenOptions::new()
            .read(true)
            .open(path.as_ref())
            .with_context(|| format!("Failed to open config file: {}", path.as_ref().display()))?;

        let config: Self = serde_yaml::from_reader(&mut f)
            .with_context(|| format!("Failed to parse config file: {}", path.as_ref().display()))?;

        return Ok(config);
    }
}
