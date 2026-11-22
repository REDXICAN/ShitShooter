//! Wire protocol shared by the client and the server.

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::weapons::WeaponId;

pub const DEFAULT_PORT: u16 = 7777;
pub const MAX_PLAYERS: usize = 16;
pub const PLAYER_COLOR_COUNT: u8 = 8;

/// renet channel ids (see `renet::DefaultChannel`).
pub const CH_UNRELIABLE: u8 = 0;
pub const CH_RELIABLE: u8 = 2;

// Gameplay constants both sides must agree on.
pub const MAX_HEALTH: i32 = 100;
/// Health comes back after this many seconds without being hurt...
pub const REGEN_DELAY: f32 = 5.0;
/// ...at this many points per second.
pub const REGEN_RATE: f32 = 20.0;
pub const RESPAWN_DELAY: f32 = 2.0;
pub const DEFAULT_FRAG_LIMIT: u32 = 20;
pub const DEFAULT_MATCH_MINUTES: f32 = 10.0;
pub const SNAPSHOT_RATE: f32 = 30.0;

// Game modes. In the team modes players are split into two teams that cannot
// hurt each other.
pub const MODE_FFA: u8 = 0;
/// Team deathmatch: every kill is a point for the team.
pub const MODE_TEAMS: u8 = 1;
/// King of the hill: a team scores a point for every second it alone holds
/// the zone in the middle of the map.
pub const MODE_HILL: u8 = 2;
pub const MODE_NAMES: [&str; 3] = ["Free for all", "Team deathmatch", "King of the hill"];
pub const TEAM_NAMES: [&str; 2] = ["Red", "Blue"];
/// The hill is everything within this distance of the map's hill point,
/// measured along the ground (so any height counts).
pub const HILL_RADIUS: f32 = 7.0;
pub const HILL_SCORE_LIMIT: u32 = 100;

/// Values of `Snapshot::hill`.
pub const HILL_EMPTY: u8 = 0;
/// Held by team 0 or team 1: `HILL_HELD + team`.
pub const HILL_HELD: u8 = 1;
pub const HILL_CONTESTED: u8 = 3;

pub fn is_team_mode(mode: u8) -> bool {
    mode != MODE_FFA
}

/// The score that wins a match: kills for one player, kills for a team, or
/// seconds on the hill.
pub fn score_limit(mode: u8, frag_limit: u32) -> u32 {
    match mode {
        MODE_TEAMS => frag_limit * 2,
        MODE_HILL => HILL_SCORE_LIMIT,
        _ => frag_limit,
    }
}

/// Grenades carried at most. One comes back every `GRENADE_RECHARGE` seconds.
pub const GRENADE_MAX: f32 = 2.0;
pub const GRENADE_RECHARGE: f32 = 12.0;
/// Seconds from the throw to the explosion.
pub const GRENADE_FUSE: f32 = 2.2;
pub const GRENADE_THROW_SPEED: f32 = 21.0;
/// Damage at the centre of the blast, falling off to nothing at `GRENADE_RADIUS`.
pub const GRENADE_DAMAGE: f32 = 130.0;
pub const GRENADE_RADIUS: f32 = 7.0;
/// Grenades are simulated in steps of exactly this long on the server and on
/// every client, so they all see the same bounces.
pub const GRENADE_STEP: f32 = 1.0 / 60.0;

/// Bits of `ClientState::flags` and `PlayerSnap::flags`.
pub const FLAG_CROUCH: u8 = 1;
pub const FLAG_RELOAD: u8 = 2;

pub type PlayerId = u64;

/// The lobby password is folded into the netcode protocol id, so a client with
/// the wrong password (or a different game version) is simply never accepted.
pub fn protocol_id(password: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in b"shitshooter-proto-4:".iter().chain(password.as_bytes()) {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerInfo {
    pub id: PlayerId,
    pub name: String,
    pub color: u8,
    /// 0 or 1. Only means something in the team modes.
    pub team: u8,
    pub kills: u32,
    pub deaths: u32,
    pub is_bot: bool,
}

/// Client -> server, reliable ordered.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ClientMsg {
    Hello { name: String },
    /// Lobby leader only.
    StartMatch,
    /// Lobby leader only.
    AddBot,
    /// Lobby leader only.
    RemoveBots,
    /// Lobby leader only, in the lobby: switch to the next map.
    NextMap,
    /// Lobby leader only, in the lobby: switch to the next game mode.
    NextMode,
    /// In the lobby: move yourself to the other team.
    SwitchTeam,
    /// Lobby leader only: abort the running match and return to the lobby.
    EndMatch,
    /// A weapon was fired: one bullet per entry of `dirs` (none for the knife).
    /// The server passes it on so that everyone else sees and hears it.
    Fire {
        weapon: WeaponId,
        origin: [f32; 3],
        dirs: Vec<[f32; 3]>,
    },
    /// One of our bullets (or our knife) reached somebody. The shooter decides
    /// what it hit, because that feels right at any ping; the server only
    /// rejects claims that are plainly impossible.
    Hit {
        victim: PlayerId,
        weapon: WeaponId,
        /// It struck the head (never for the knife).
        headshot: bool,
    },
    /// Throw a grenade. The server simulates it and deals the damage.
    Grenade { origin: [f32; 3], vel: [f32; 3] },
}

/// Client -> server, unreliable, sent every fixed tick while alive.
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct ClientState {
    pub pos: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub flags: u8,
    /// The weapon in hand.
    pub weapon: WeaponId,
    /// Number of spawns this client has processed. Lets the server ignore stale
    /// positions that were sent before a respawn teleport arrived.
    pub spawn_seq: u32,
}

/// Server -> client, reliable ordered.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ServerMsg {
    Welcome {
        id: PlayerId,
    },
    Roster {
        players: Vec<PlayerInfo>,
        leader: PlayerId,
        /// Index into `map::MAP_NAMES`.
        map: u8,
        /// One of the `MODE_*` constants.
        mode: u8,
    },
    MatchStarted {
        /// See `score_limit`.
        score_limit: u32,
        time_left: f32,
    },
    MatchEnded {
        winner: Winner,
    },
    Spawn {
        id: PlayerId,
        pos: [f32; 3],
        yaw: f32,
        spawn_seq: u32,
    },
    /// Somebody else fired. Sent to everyone but the shooter.
    Fire {
        shooter: PlayerId,
        weapon: WeaponId,
        origin: [f32; 3],
        dirs: Vec<[f32; 3]>,
    },
    Hit {
        victim: PlayerId,
        attacker: PlayerId,
        health: i32,
    },
    Kill {
        killer: PlayerId,
        victim: PlayerId,
        /// What did it; `weapons::GRENADE` for a grenade.
        weapon: WeaponId,
    },
    Grenade {
        id: u32,
        pos: [f32; 3],
        vel: [f32; 3],
    },
    Explosion {
        /// The grenade that went off.
        id: u32,
        pos: [f32; 3],
    },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum Winner {
    Nobody,
    Player(PlayerId),
    Team(u8),
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct PlayerSnap {
    pub id: PlayerId,
    pub pos: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub flags: u8,
    pub weapon: WeaponId,
    pub health: i32,
    pub alive: bool,
}

/// Server -> client, unreliable, `SNAPSHOT_RATE` times per second during a match.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Snapshot {
    pub time_left: f32,
    /// Team modes: each team's score.
    pub team_scores: [u32; 2],
    /// King of the hill: who holds the hill (`HILL_*`).
    pub hill: u8,
    pub players: Vec<PlayerSnap>,
}

pub fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    postcard::to_stdvec(value).expect("message serialization cannot fail")
}

pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    postcard::from_bytes(bytes).ok()
}
