use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::ops::Deref;
use std::sync::Arc;

use anyhow::Result;
use futures::SinkExt;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::StreamExt;
use tokio_util::codec::{Framed, LinesCodec};
use tracing::{debug, error, info};

use crate::config::Named;
use crate::sink::Sink;
use crate::source::Source;

pub struct State {
    pub sources: HashMap<Arc<String>, Named<Source>>,
    pub sinks: HashMap<Arc<String>, Named<Sink>>,
}

const JSONRPC_TAG: &'static str = "2.0";

pub async fn serve(state: State) -> Result<()> {
    let listener = TcpListener::bind("[::]:1705").await?;

    let shared = Arc::new(Mutex::new(Shared {
        clients: HashMap::new(),
        state: Arc::new(Mutex::new(state)),
    }));

    loop {
        let (stream, addr) = listener.accept().await?;

        let shared = shared.clone();

        tokio::spawn(async move {
            debug!("Accepted connection: {}", addr);
            if let Err(e) = process(shared, stream, addr).await {
                info!("Error occurred: {}", e);
            }
        });
    }
}

async fn process(shared: Arc<Mutex<Shared>>, stream: TcpStream, addr: SocketAddr) -> Result<()> {
    let (tx, mut rx) = mpsc::channel(16);

    // Register this client for broadcasting
    shared.lock().await.clients.insert(addr, tx.clone());

    // Framer codec for line based protocol
    let mut lines = Framed::new(stream, LinesCodec::new());

    // Process incoming messages until disconnected
    loop {
        tokio::select! {
            Some(res) = rx.recv() => {
                // Encode response to JSON
                let res = match serde_json::to_string(&res) {
                    Ok(res) => res,
                    Err(err) => {
                        error!("Protocol error: {}", err);
                        break;
                    }
                };

                debug!("Response to send: {:?}", res);

                // Send response line to client
                if let Err(err) = lines.send(&res).await {
                    error!("TCP error: {}", err);
                    break;
                }
            }

            result = lines.next() => match result {
                Some(Ok(req)) => {
                    let req = req.trim();
                    if req.is_empty() {
                        continue;
                    }

                    debug!("Parse request: {}", req);

                    let mut shared = shared.lock().await;

                    let res = match serde_json::from_str::<Request>(&req) {
                        Ok(req) => match shared.dispatch(&req).await {
                            Ok(res) => match serde_json::to_value(res) {
                                Ok(res) => req.id.and_then(|id| Some(Response::ok(res).with_id(Some(id)))),
                                Err(err) => {
                                    error!("Protocol error: {}", err);
                                    break;
                                }
                            }
                            Err(err) => Some(Response::error(err).with_id(req.id)),
                        }
                        Err(err) => Some(Response::error(ResponseError::parse_error(err)))
                    };

                    if let Some(res) = res {
                        debug!("Dispatch response: {:?}", res);
                        tx.send(res).await
                                .expect("Send response");
                    }
                }

                Some(Err(err)) => {
                    error!("TCP error: {}", err);
                    break;
                }

                // Incoming stream has been exhausted
                None => break
            }
        }
    }

    // Client has disconnected - deregister
    shared.lock().await.clients.remove(&addr);

    return Ok(());
}

#[derive(Deserialize, Debug)]
struct Request {
    #[serde(rename = "jsonrpc")]
    #[allow(unused)]
    tag: String,

    id: Option<String>,

    method: String,

    #[serde(default)]
    params: Map<String, Value>,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
struct ResponseError {
    pub code: i32,
    pub message: String,
    pub data: Value,
}

impl ResponseError {
    pub fn error(code: i32, error: impl Into<anyhow::Error>) -> Self {
        return Self::message(code, error.into().to_string());
    }

    pub fn message(code: i32, message: String) -> Self {
        return Self {
            code,
            message,
            data: Value::Null,
        };
    }

    pub fn parse_error(error: impl Into<anyhow::Error>) -> Self {
        return Self::message(-32700, format!("Parse error: {}", error.into()));
    }

    #[allow(unused)]
    pub fn invalid_request(request: String) -> Self {
        return Self::message(-32600, format!("Invalid request: {}", request));
    }

    #[allow(unused)]
    pub fn method_not_found(method: String) -> Self {
        return Self::message(-32601, format!("Method not found: {}", method));
    }

    pub fn invalid_params(message: String) -> Self {
        return Self::message(-32602, format!("Invalid parameter: {}", message));
    }

    #[allow(unused)]
    pub fn internal(error: impl Into<anyhow::Error>) -> Self {
        return Self::message(-32603, error.into().to_string());
    }
}

#[derive(Serialize, Debug, PartialEq, Eq)]
enum ResponseData {
    #[serde(rename = "result")]
    Result(Value),
    #[serde(rename = "error")]
    Error(ResponseError),
}

#[derive(Serialize, Debug, PartialEq, Eq)]
struct Response {
    #[serde(rename = "jsonrpc")]
    pub tag: &'static str,

    pub id: Option<String>,

    #[serde(flatten)]
    pub data: ResponseData,
}

impl Response {
    pub fn ok(value: Value) -> Self {
        return Self {
            tag: JSONRPC_TAG,
            id: None,
            data: ResponseData::Result(value),
        };
    }

    pub fn error(error: ResponseError) -> Self {
        return Self {
            tag: JSONRPC_TAG,
            id: None,
            data: ResponseData::Error(error),
        };
    }

    pub fn with_id(mut self, id: Option<String>) -> Self {
        self.id = id;
        return self;
    }
}

struct Shared {
    clients: HashMap<SocketAddr, mpsc::Sender<Response>>,

    state: Arc<Mutex<State>>,
}

impl Shared {
    async fn dispatch(&mut self, req: &Request) -> Result<Value, ResponseError> {
        async fn dispatch<'a, F, P, R, A>(
            shared: &'a mut Shared,
            request: &Request,
            f: F,
        ) -> Result<Value, ResponseError>
        where
            F: FnOnce(&'a mut Shared, P) -> A,
            A: Future<Output = Result<R, ResponseError>> + 'a,
            P: DeserializeOwned,
            R: Serialize,
        {
            let params = serde_json::from_value(request.params.clone().into())
                .map_err(|err| ResponseError::error(-32602, err))?;

            let value = f(shared, params).await?;

            return Ok(
                serde_json::to_value(value).map_err(|err| ResponseError::error(-32603, err))?
            );
        }

        return Ok(match req.method.as_str() {
            "Client.GetStatus" => dispatch(self, req, Self::client_get_status).await?,
            "Client.SetVolume" => dispatch(self, req, Self::client_set_volume).await?,
            "Group.GetStatus" => dispatch(self, req, Self::group_get_status).await?,
            "Group.SetMute" => dispatch(self, req, Self::group_set_mute).await?,
            "Group.SetStream" => dispatch(self, req, Self::group_set_stream).await?,
            "Server.GetRPCVersion" => dispatch(self, req, Self::server_get_rpc_version).await?,
            "Server.GetStatus" => dispatch(self, req, Self::server_get_status).await?,
            _ => {
                return Err(ResponseError::message(
                    -32601,
                    format!("Method not found: {}", req.method),
                ))
            }
        });
    }

    async fn client_get_status(
        &mut self,
        params: WithId<types::Empty>,
    ) -> Result<types::Client, ResponseError> {
        let state = self.state.lock().await;

        let sink = state.sinks.get(&params.id).ok_or_else(|| {
            ResponseError::invalid_params(format!("Unknown client: {}", params.id))
        })?;

        return Ok(types::Client::from(sink));
    }

    async fn client_set_volume(
        &mut self,
        params: WithId<types::Volume>,
    ) -> Result<types::Volume, ResponseError> {
        let mut state = self.state.lock().await;

        let sink = state.sinks.get_mut(&params.id).ok_or_else(|| {
            ResponseError::invalid_params(format!("Unknown client: {}", params.id))
        })?;

        sink.set_muted(params.muted);
        sink.set_volume((params.percent / 100.0 * u8::MAX as f32) as u8);

        return Ok(params.inner);
    }

    async fn group_get_status(
        &mut self,
        params: WithId<types::Empty>,
    ) -> Result<types::Group, ResponseError> {
        let state = self.state.lock().await;

        // We have one group per sink, therefor the group id is equal to the sink id
        let sink = state.sinks.get(&params.id).ok_or_else(|| {
            ResponseError::invalid_params(format!("Unknown group: {}", params.id))
        })?;

        return Ok(types::Group::from(sink));
    }

    async fn group_set_mute(
        &mut self,
        params: WithId<types::Mute>,
    ) -> Result<types::Mute, ResponseError> {
        let mut state = self.state.lock().await;

        // We have one group per sink, therefor the group id is equal to the sink id
        let sink = state.sinks.get_mut(&params.id).ok_or_else(|| {
            ResponseError::invalid_params(format!("Unknown group: {}", params.id))
        })?;

        sink.set_muted(params.mute);

        return Ok(types::Mute { mute: params.mute });
    }

    async fn group_set_stream(
        &mut self,
        params: WithId<types::StreamId>,
    ) -> Result<types::Stream, ResponseError> {
        let state = &mut *self.state.lock().await;

        // We have one group per sink, therefor the group id is equal to the sink id
        let sink = state.sinks.get_mut(&params.id).ok_or_else(|| {
            ResponseError::invalid_params(format!("Unknown group: {}", params.id))
        })?;

        let source = state.sources.get(&params.stream_id).ok_or_else(|| {
            ResponseError::invalid_params(format!("Unknown stream: {}", params.stream_id))
        })?;

        let control = sink.get_source(&source.name).ok_or_else(|| {
            ResponseError::invalid_params(format!("Unknown stream: {}", params.stream_id))
        })?;

        control.switch();

        return Ok(types::Stream::from(source));
    }

    async fn server_get_rpc_version(
        &mut self,
        _params: types::Empty,
    ) -> Result<types::Version, ResponseError> {
        return Ok(types::Version::default());
    }

    async fn server_get_status(
        &mut self,
        _params: types::Empty,
    ) -> Result<types::Server, ResponseError> {
        let state = self.state.lock().await;

        // Create a group for each sink
        let groups = state
            .sinks
            .values()
            .map(types::Group::from)
            .collect::<Vec<_>>();

        let streams = state
            .sources
            .values()
            .map(types::Stream::from)
            .collect::<Vec<_>>();

        return Ok(types::Server {
            server: types::ServerInner {
                host: types::Host::default(),
                meta: types::Meta::default(),
            },
            groups,
            streams,
        });
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct WithId<I> {
    pub id: String,

    #[serde(flatten)]
    pub inner: I,
}

impl<I> Deref for WithId<I> {
    type Target = I;

    fn deref(&self) -> &Self::Target {
        return &self.inner;
    }
}

mod types {
    use std::sync::Arc;
    use std::time::SystemTime;

    use serde::{Deserialize, Serialize};
    use url::Url;

    use crate::config::Named;
    use crate::sink::Sink;
    use crate::source::Source;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Empty {}

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Volume {
        pub muted: bool,
        pub percent: f32,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Host {}

    impl Default for Host {
        fn default() -> Self {
            return Self {};
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Timestamp {
        pub sec: u64,
        pub usec: u32,
    }

    impl Timestamp {
        pub fn now() -> Self {
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("clock error");

            return Self {
                sec: now.as_secs(),
                usec: now.subsec_micros(),
            };
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ClientConfig {
        pub name: String,
        pub instance: u32,
        pub latency: u32,
        pub volume: Volume,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Meta {
        pub name: String,
        #[serde(rename = "protocolVersion")]
        pub protocol: u32,
        pub version: String,
    }

    impl Default for Meta {
        fn default() -> Self {
            return Self {
                name: env!("CARGO_PKG_NAME").to_string(),
                protocol: 2,
                version: env!("CARGO_PKG_VERSION").to_string(),
            };
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Client {
        pub id: Arc<String>,
        pub connected: bool,
        pub host: Host,
        #[serde(rename = "lastSeen")]
        pub last_seen: Timestamp,
        pub config: ClientConfig,
        #[serde(rename = "snapclient")]
        pub meta: Meta,
    }

    impl Client {
        pub fn from(sink: &Named<Sink>) -> Self {
            return Self {
                id: sink.name.clone(),
                connected: true,
                host: Host::default(),
                last_seen: Timestamp::now(),
                config: ClientConfig {
                    name: sink.name().to_string(),
                    instance: 0,
                    latency: 0,
                    volume: Volume {
                        muted: sink.muted(),
                        percent: sink.volume() as f32 / u8::MAX as f32 * 100.0,
                    },
                },
                meta: Default::default(),
            };
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Group {
        pub id: Arc<String>,
        pub name: Arc<String>,
        pub muted: bool,
        pub clients: Vec<Client>,
        pub stream_id: Arc<String>,
    }

    impl Group {
        pub fn from(sink: &Named<Sink>) -> Self {
            return Self {
                id: sink.name.clone(),
                name: sink.name.clone(),
                muted: sink.muted(),
                clients: vec![Client::from(sink)],
                stream_id: sink
                    .get_active_source()
                    .map(|(name, _)| name)
                    .unwrap_or_default(),
            };
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Mute {
        pub mute: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct StreamId {
        pub stream_id: Arc<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum StreamStatus {
        #[serde(rename = "idle")]
        Idle,

        #[serde(rename = "active")]
        Active,
    }

    impl From<bool> for StreamStatus {
        fn from(value: bool) -> Self {
            return match value {
                true => Self::Active,
                false => Self::Idle,
            };
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Stream {
        pub stream_id: Arc<String>,
        pub status: StreamStatus,
        pub uri: Url,
    }

    impl Stream {
        pub fn from(source: &Named<Source>) -> Self {
            return Self {
                stream_id: source.name.clone(),
                status: source.is_active().into(),
                uri: source.uri(),
            };
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Version {
        pub major: u32,
        pub minor: u32,
        pub patch: u32,
    }

    impl Default for Version {
        fn default() -> Self {
            return Self {
                major: 2,
                minor: 0,
                patch: 0,
            };
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ServerInner {
        pub host: Host,
        #[serde(rename = "snapserver")]
        pub meta: Meta,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Server {
        pub server: ServerInner,
        pub groups: Vec<Group>,
        pub streams: Vec<Stream>,
    }
}
