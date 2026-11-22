//! Client-side networking: the connection to the server (ours or someone
//! else's) and the session state that the server tells us about.

use std::{
    net::{Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket},
    time::SystemTime,
};

use bevy::prelude::*;
use renet::{ConnectionConfig, RenetClient};
use renet_netcode::{ClientAuthentication, NetcodeClientTransport};

use crate::{
    AppState,
    map::Map,
    protocol::*,
    server::{Server, ServerHandle, ServerSettings},
    world::GameMap,
};

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Session>()
            .init_resource::<MenuStatus>()
            .add_message::<NetMsg>()
            .add_message::<SnapshotMsg>()
            .add_systems(
                PreUpdate,
                (receive, update_session)
                    .chain()
                    .run_if(resource_exists::<Connection>),
            )
            .add_systems(PostUpdate, flush.run_if(resource_exists::<Connection>));
    }
}

/// A reliable message from the server.
#[derive(Message)]
pub struct NetMsg(pub ServerMsg);

/// A world snapshot from the server.
#[derive(Message)]
pub struct SnapshotMsg(pub Snapshot);

/// Error or info line shown on the main menu.
#[derive(Resource, Default)]
pub struct MenuStatus(pub String);

/// The server running inside this process when we are the host.
#[derive(Resource)]
pub struct HostedServer(#[allow(dead_code)] ServerHandle);

/// What we know about the lobby/match we are connected to.
#[derive(Resource, Default)]
pub struct Session {
    pub my_id: PlayerId,
    pub leader: PlayerId,
    pub roster: Vec<PlayerInfo>,
    /// One of the `MODE_*` constants.
    pub mode: u8,
    /// The score that wins: kills, team kills or seconds on the hill.
    pub score_limit: u32,
    pub team_scores: [u32; 2],
    /// Who holds the hill (`HILL_*`).
    pub hill: u8,
    pub time_left: f32,
    /// Result line of the previous match, shown in the lobby.
    pub last_result: Option<String>,
    /// Address to tell friends about, when hosting.
    pub share_address: Option<String>,
    pub server_label: String,
}

impl Session {
    pub fn is_leader(&self) -> bool {
        self.my_id != 0 && self.my_id == self.leader
    }

    pub fn name_of(&self, id: PlayerId) -> &str {
        self.roster
            .iter()
            .find(|p| p.id == id)
            .map_or("???", |p| p.name.as_str())
    }

    pub fn team_mode(&self) -> bool {
        is_team_mode(self.mode)
    }

    pub fn team_of(&self, id: PlayerId) -> u8 {
        self.roster.iter().find(|p| p.id == id).map_or(0, |p| p.team & 1)
    }

    /// True if `a` and `b` can hurt each other: two different players who are
    /// not on the same team.
    pub fn enemies(&self, a: PlayerId, b: PlayerId) -> bool {
        a != b && !(self.team_mode() && self.team_of(a) == self.team_of(b))
    }
}

#[derive(Resource)]
pub struct Connection {
    client: RenetClient,
    transport: NetcodeClientTransport,
    name: String,
    said_hello: bool,
}

impl Connection {
    pub fn open(server_addr: SocketAddr, password: &str, name: &str) -> Result<Connection, String> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).map_err(|e| e.to_string())?;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let authentication = ClientAuthentication::Unsecure {
            protocol_id: protocol_id(password),
            // Client ids double as player ids; a clock-derived id is unique enough for a lobby.
            client_id: (now.as_nanos() as u64 ^ (std::process::id() as u64).rotate_left(40)) | (1 << 32),
            server_addr,
            user_data: None,
        };
        let transport =
            NetcodeClientTransport::new(now, authentication, socket).map_err(|e| e.to_string())?;
        Ok(Connection {
            client: RenetClient::new(ConnectionConfig::default()),
            transport,
            name: name.to_string(),
            said_hello: false,
        })
    }

    pub fn send(&mut self, msg: &ClientMsg) {
        self.client.send_message(CH_RELIABLE, encode(msg));
    }

    pub fn send_state(&mut self, state: &ClientState) {
        self.client.send_message(CH_UNRELIABLE, encode(state));
    }

    /// Round trip time in milliseconds.
    pub fn ping_ms(&self) -> f64 {
        self.client.rtt() * 1000.0
    }
}

/// Starts a server in this process and connects to it.
pub fn host(commands: &mut Commands, settings: ServerSettings, name: &str) -> Result<(), String> {
    let port = settings.port;
    let password = settings.password.clone();
    let server = Server::bind(settings)
        .map_err(|e| format!("Could not host on port {port}: {e}"))?;
    let connection = Connection::open(SocketAddr::from((Ipv4Addr::LOCALHOST, port)), &password, name)?;
    commands.insert_resource(HostedServer(server.spawn_thread()));
    commands.insert_resource(connection);
    commands.insert_resource(Session {
        share_address: Some(match lan_ip() {
            Some(ip) => format!("{ip}:{port}"),
            None => format!("<your ip>:{port}"),
        }),
        server_label: "your own lobby".to_string(),
        ..default()
    });
    Ok(())
}

/// Connects to somebody else's server. `address` may omit the port.
pub fn join(commands: &mut Commands, address: &str, password: &str, name: &str) -> Result<(), String> {
    let address = address.trim();
    let with_port = if address.contains(':') {
        address.to_string()
    } else {
        format!("{address}:{DEFAULT_PORT}")
    };
    let server_addr = with_port
        .to_socket_addrs()
        .ok()
        .and_then(|mut addrs| addrs.find(SocketAddr::is_ipv4))
        .ok_or_else(|| format!("'{address}' is not a valid address"))?;
    commands.insert_resource(Connection::open(server_addr, password, name)?);
    commands.insert_resource(Session {
        server_label: with_port,
        ..default()
    });
    Ok(())
}

/// Closes the connection (and the hosted server, if any).
pub fn disconnect(commands: &mut Commands, connection: Option<&mut Connection>) {
    if let Some(connection) = connection {
        connection.transport.disconnect();
    }
    commands.remove_resource::<Connection>();
    commands.remove_resource::<HostedServer>();
}

/// The address other machines on the local network can reach us at.
/// "Connecting" a UDP socket sends nothing; it only makes the OS pick a route.
fn lan_ip() -> Option<String> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(8, 8, 8, 8), 80)).ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then(|| ip.to_string())
}

fn receive(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut connection: ResMut<Connection>,
    mut reliable: MessageWriter<NetMsg>,
    mut snapshots: MessageWriter<SnapshotMsg>,
    mut status: ResMut<MenuStatus>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let connection = &mut *connection;
    let delta = time.delta();
    connection.client.update(delta);
    let transport_result = connection.transport.update(delta, &mut connection.client);

    if transport_result.is_err() || connection.client.is_disconnected() {
        let never_connected = !connection.said_hello;
        status.0 = if never_connected {
            "Could not connect. Check the address, the password, and that the host's UDP port is reachable.".into()
        } else {
            "Disconnected from the server.".into()
        };
        disconnect(&mut commands, Some(connection));
        next_state.set(AppState::Menu);
        return;
    }
    if !connection.client.is_connected() {
        return;
    }
    if !connection.said_hello {
        connection.said_hello = true;
        let name = connection.name.clone();
        connection.send(&ClientMsg::Hello { name });
    }

    while let Some(bytes) = connection.client.receive_message(CH_RELIABLE) {
        if let Some(msg) = decode::<ServerMsg>(&bytes) {
            reliable.write(NetMsg(msg));
        }
    }
    // Only the newest snapshot matters.
    let mut latest = None;
    while let Some(bytes) = connection.client.receive_message(CH_UNRELIABLE) {
        latest = decode::<Snapshot>(&bytes).or(latest);
    }
    if let Some(snapshot) = latest {
        snapshots.write(SnapshotMsg(snapshot));
    }
}

fn update_session(
    mut messages: MessageReader<NetMsg>,
    mut snapshots: MessageReader<SnapshotMsg>,
    mut session: ResMut<Session>,
    mut game_map: ResMut<GameMap>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for NetMsg(msg) in messages.read() {
        match msg {
            ServerMsg::Welcome { id } => {
                session.my_id = *id;
                next_state.set(AppState::Lobby);
            }
            ServerMsg::Roster {
                players,
                leader,
                map,
                mode,
            } => {
                session.roster = players.clone();
                session.leader = *leader;
                session.mode = *mode;
                // The host picked another map: swap the scenery and the collision.
                if game_map.0.id != *map {
                    game_map.0 = Map::by_id(*map);
                }
            }
            ServerMsg::MatchStarted { score_limit, time_left } => {
                session.score_limit = *score_limit;
                session.team_scores = [0, 0];
                session.hill = HILL_EMPTY;
                session.time_left = *time_left;
                next_state.set(AppState::Playing);
            }
            ServerMsg::MatchEnded { winner } => {
                session.last_result = Some(match winner {
                    Winner::Player(id) => format!("{} won the last match!", session.name_of(*id)),
                    Winner::Team(team) => format!(
                        "Team {} won the last match, {} to {}!",
                        TEAM_NAMES[*team as usize & 1],
                        session.team_scores[*team as usize & 1],
                        session.team_scores[!*team as usize & 1],
                    ),
                    Winner::Nobody => "The last match ended without a winner.".to_string(),
                });
                next_state.set(AppState::Lobby);
            }
            _ => {}
        }
    }
    for SnapshotMsg(snapshot) in snapshots.read() {
        session.time_left = snapshot.time_left;
        session.team_scores = snapshot.team_scores;
        session.hill = snapshot.hill;
    }
}

fn flush(mut connection: ResMut<Connection>) {
    let connection = &mut *connection;
    // Errors surface as a disconnect on the next `receive`.
    let _ = connection.transport.send_packets(&mut connection.client);
}
