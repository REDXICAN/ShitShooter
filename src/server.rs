//! The game server. It is plain Rust with no ECS: it runs on its own thread
//! inside the host's game process, or on the main thread with `--server`.

use std::{
    collections::HashMap,
    io,
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime},
};

use bevy::math::Vec3;
use renet::{ConnectionConfig, RenetServer, ServerEvent};
use renet_netcode::{NetcodeServerTransport, ServerAuthentication, ServerConfig};

use crate::{
    map::{self, Map},
    protocol::*,
    weapons::{self, KNIFE, KNIFE_RANGE, WEAPONS, WeaponId, hit_damage, weapon},
};

const TICK: Duration = Duration::from_micros(1_000_000 / 60);
const BOT_NAMES: [&str; 8] = [
    "Clanker", "Tinhead", "Bolt", "Rusty", "Sprocket", "Gizmo", "Widget", "Scrap",
];

#[derive(Clone)]
pub struct ServerSettings {
    pub port: u16,
    pub password: String,
    pub frag_limit: u32,
    pub match_minutes: f32,
    pub bots: usize,
    /// Index into `map::MAP_NAMES`.
    pub map: u8,
    /// One of the `MODE_*` constants.
    pub mode: u8,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            password: String::new(),
            frag_limit: DEFAULT_FRAG_LIMIT,
            match_minutes: DEFAULT_MATCH_MINUTES,
            bots: 0,
            map: 0,
            mode: MODE_FFA,
        }
    }
}

struct Bot {
    vel: Vec3,
    target: Vec3,
    retarget_in: f32,
}

struct Player {
    name: String,
    color: u8,
    team: u8,
    kills: u32,
    deaths: u32,
    join_order: u64,
    alive: bool,
    health: i32,
    pos: Vec3,
    yaw: f32,
    pitch: f32,
    /// `FLAG_*` bits reported by the client.
    flags: u8,
    /// The weapon in hand, as reported by the client.
    weapon: WeaponId,
    respawn_in: f32,
    spawn_seq: u32,
    /// Seconds of firing this player has saved up. Every shot costs most of
    /// the weapon's interval, so nobody can fire faster than their gun allows,
    /// yet two shots arriving in one tick (network jitter) are both accepted.
    fire_credit: f32,
    /// Bullets fired that have not hit anything yet. A hit is only accepted
    /// while this is above zero.
    bullets_out: u32,
    /// A knife swing was announced and has not been used for a hit yet.
    stab_ready: bool,
    since_damage: f32,
    /// Fraction of a health point regenerated so far.
    regen: f32,
    /// Grenades in hand, counted the same way the client counts them.
    grenades: f32,
    bot: Option<Bot>,
}

const MAX_FIRE_CREDIT: f32 = 1.5;
const MAX_BULLETS_OUT: u32 = 40;

struct Grenade {
    id: u32,
    owner: PlayerId,
    pos: Vec3,
    vel: Vec3,
    fuse: f32,
}

#[derive(PartialEq)]
enum Phase {
    Lobby,
    Playing,
}

pub struct Server {
    renet: RenetServer,
    transport: NetcodeServerTransport,
    settings: ServerSettings,
    map: Map,
    players: HashMap<PlayerId, Player>,
    phase: Phase,
    time_left: f32,
    join_counter: u64,
    next_bot_id: PlayerId,
    snapshot_timer: f32,
    roster_dirty: bool,
    rng: u64,
    grenades: Vec<Grenade>,
    next_grenade_id: u32,
    /// Time not yet consumed by whole grenade steps.
    grenade_clock: f32,
    /// Team modes: kills (team deathmatch) or seconds on the hill.
    team_scores: [u32; 2],
    /// Who holds the hill right now (`HILL_*`).
    hill: u8,
    /// Time held that has not become a whole point yet.
    hill_clock: f32,
}

impl Server {
    /// Binds the UDP socket. Fails if the port is already in use.
    pub fn bind(settings: ServerSettings) -> io::Result<Server> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, settings.port))?;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let config = ServerConfig {
            current_time: now,
            max_clients: MAX_PLAYERS,
            protocol_id: protocol_id(&settings.password),
            public_addresses: vec![SocketAddr::from((Ipv4Addr::LOCALHOST, settings.port))],
            authentication: ServerAuthentication::Unsecure,
        };
        let transport = NetcodeServerTransport::new(config, socket)?;
        let mut server = Server {
            renet: RenetServer::new(ConnectionConfig::default()),
            transport,
            map: Map::by_id(settings.map),
            grenades: Vec::new(),
            next_grenade_id: 0,
            grenade_clock: 0.0,
            team_scores: [0, 0],
            hill: HILL_EMPTY,
            hill_clock: 0.0,
            players: HashMap::new(),
            phase: Phase::Lobby,
            time_left: 0.0,
            join_counter: 0,
            next_bot_id: 1,
            snapshot_timer: 0.0,
            roster_dirty: false,
            rng: now.as_nanos() as u64 | 1,
            settings,
        };
        for _ in 0..server.settings.bots {
            server.add_bot();
        }
        Ok(server)
    }

    /// Runs the server on a background thread until the returned handle is dropped.
    pub fn spawn_thread(self) -> ServerHandle {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread = thread::Builder::new()
            .name("game-server".into())
            .spawn(move || self.run(&thread_stop))
            .expect("failed to spawn the server thread");
        ServerHandle {
            stop,
            thread: Some(thread),
        }
    }

    /// Runs the server loop until `stop` is set.
    pub fn run(mut self, stop: &AtomicBool) {
        let mut last = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            let now = Instant::now();
            let dt = now - last;
            last = now;
            self.tick(dt);
            thread::sleep(TICK.saturating_sub(now.elapsed()));
        }
        self.transport.disconnect_all(&mut self.renet);
    }

    fn tick(&mut self, delta: Duration) {
        let dt = delta.as_secs_f32().min(0.25);
        self.renet.update(delta);
        if let Err(err) = self.transport.update(delta, &mut self.renet) {
            eprintln!("[server] transport error: {err}");
        }

        while let Some(event) = self.renet.get_event() {
            match event {
                ServerEvent::ClientConnected { client_id } => {
                    println!("[server] client {client_id:016x} connected");
                }
                ServerEvent::ClientDisconnected { client_id, reason } => {
                    if let Some(player) = self.players.remove(&client_id) {
                        println!("[server] {} left ({reason})", player.name);
                        self.roster_dirty = true;
                    }
                }
            }
        }

        for client_id in self.renet.clients_id() {
            while let Some(bytes) = self.renet.receive_message(client_id, CH_RELIABLE) {
                match decode::<ClientMsg>(&bytes) {
                    Some(msg) => self.handle_message(client_id, msg),
                    None => eprintln!("[server] bad message from {client_id:016x}"),
                }
            }
            let mut latest = None;
            while let Some(bytes) = self.renet.receive_message(client_id, CH_UNRELIABLE) {
                latest = decode::<ClientState>(&bytes).or(latest);
            }
            if let (Some(state), Some(player)) = (latest, self.players.get_mut(&client_id))
                && player.alive
                && state.spawn_seq == player.spawn_seq
                && state.pos.iter().all(|v| v.is_finite())
            {
                player.pos = Vec3::from(state.pos);
                player.yaw = state.yaw;
                player.pitch = state.pitch;
                player.flags = state.flags;
                player.weapon = state.weapon;
            }
        }

        // With nobody left to play, fall back to the lobby (drops leftover bots' match).
        if self.phase == Phase::Playing && self.players.values().all(|p| p.bot.is_some()) {
            self.end_match(Winner::Nobody);
        }

        if self.phase == Phase::Playing {
            self.update_match(dt);
        }

        if self.roster_dirty {
            self.roster_dirty = false;
            let roster = self.roster_msg();
            self.broadcast(&roster);
        }
        self.transport.send_packets(&mut self.renet);
    }

    fn update_match(&mut self, dt: f32) {
        self.time_left -= dt;

        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        for id in &ids {
            let player = self.players.get_mut(id).unwrap();
            player.fire_credit = (player.fire_credit + dt).min(MAX_FIRE_CREDIT);
            player.grenades = (player.grenades + dt / GRENADE_RECHARGE).min(GRENADE_MAX);
            // Health comes back after a while out of trouble.
            player.since_damage += dt;
            if player.alive && player.health < MAX_HEALTH && player.since_damage >= REGEN_DELAY {
                player.regen += REGEN_RATE * dt;
                let whole = player.regen.floor();
                player.regen -= whole;
                player.health = (player.health + whole as i32).min(MAX_HEALTH);
            }
            if !player.alive {
                player.respawn_in -= dt;
                if player.respawn_in <= 0.0 {
                    self.spawn_player(*id);
                }
            }
        }
        self.update_bots(dt);
        self.update_grenades(dt);
        if self.settings.mode == MODE_HILL && self.phase == Phase::Playing {
            self.update_hill(dt);
        }
        if self.phase != Phase::Playing {
            // The last explosion (or second on the hill) won the match.
            return;
        }

        self.snapshot_timer += dt;
        if self.snapshot_timer >= 1.0 / SNAPSHOT_RATE {
            self.snapshot_timer = 0.0;
            let snapshot = Snapshot {
                time_left: self.time_left.max(0.0),
                team_scores: self.team_scores,
                hill: self.hill,
                players: self
                    .players
                    .iter()
                    .map(|(id, p)| PlayerSnap {
                        id: *id,
                        pos: p.pos.to_array(),
                        yaw: p.yaw,
                        pitch: p.pitch,
                        flags: p.flags,
                        weapon: p.weapon,
                        health: p.health,
                        alive: p.alive,
                    })
                    .collect(),
            };
            self.renet.broadcast_message(CH_UNRELIABLE, encode(&snapshot));
        }

        if self.time_left <= 0.0 {
            // Time is up: the best score wins, a tie has no winner.
            let winner = if is_team_mode(self.settings.mode) {
                match self.team_scores[0].cmp(&self.team_scores[1]) {
                    std::cmp::Ordering::Greater => Winner::Team(0),
                    std::cmp::Ordering::Less => Winner::Team(1),
                    std::cmp::Ordering::Equal => Winner::Nobody,
                }
            } else {
                let best = self.players.values().map(|p| p.kills).max().unwrap_or(0);
                let mut leaders = self.players.iter().filter(|(_, p)| p.kills == best);
                match (leaders.next(), leaders.next()) {
                    (Some((id, _)), None) => Winner::Player(*id),
                    _ => Winner::Nobody,
                }
            };
            self.end_match(winner);
        }
    }

    /// King of the hill: a team alone on the hill scores a point every second.
    fn update_hill(&mut self, dt: f32) {
        let mut present = [false; 2];
        for player in self.players.values() {
            let from_hill = (player.pos - self.map.hill).with_y(0.0).length();
            if player.alive && from_hill < HILL_RADIUS {
                present[player.team as usize & 1] = true;
            }
        }
        let holder = match present {
            [true, true] => None,
            [true, false] => Some(0),
            [false, true] => Some(1),
            [false, false] => None,
        };
        self.hill = match (holder, present[0]) {
            (Some(team), _) => HILL_HELD + team as u8,
            (None, true) => HILL_CONTESTED,
            (None, false) => HILL_EMPTY,
        };
        let Some(team) = holder else {
            return;
        };
        self.hill_clock += dt;
        while self.hill_clock >= 1.0 {
            self.hill_clock -= 1.0;
            self.team_scores[team] += 1;
        }
        if self.team_scores[team] >= HILL_SCORE_LIMIT {
            self.end_match(Winner::Team(team as u8));
        }
    }

    fn handle_message(&mut self, id: PlayerId, msg: ClientMsg) {
        let is_leader = self.leader() == Some(id);
        match msg {
            ClientMsg::Hello { name } => {
                if self.players.contains_key(&id) {
                    return;
                }
                let name = sanitize_name(&name);
                println!("[server] {name} joined");
                self.join_counter += 1;
                let player = self.new_player(name, None);
                self.players.insert(id, player);
                self.send(id, &ServerMsg::Welcome { id });
                // Roster first, so the newcomer knows everyone before any spawn arrives.
                let roster = self.roster_msg();
                self.broadcast(&roster);
                if self.phase == Phase::Playing {
                    self.send(
                        id,
                        &ServerMsg::MatchStarted {
                            score_limit: score_limit(self.settings.mode, self.settings.frag_limit),
                            time_left: self.time_left,
                        },
                    );
                    self.spawn_player(id);
                }
            }
            ClientMsg::StartMatch if is_leader && self.phase == Phase::Lobby => self.start_match(),
            ClientMsg::EndMatch if is_leader && self.phase == Phase::Playing => self.end_match(Winner::Nobody),
            ClientMsg::NextMode if is_leader && self.phase == Phase::Lobby => {
                self.settings.mode = (self.settings.mode + 1) % MODE_NAMES.len() as u8;
                self.roster_dirty = true;
            }
            ClientMsg::SwitchTeam if self.phase == Phase::Lobby => {
                if let Some(player) = self.players.get_mut(&id) {
                    player.team ^= 1;
                    self.roster_dirty = true;
                }
            }
            ClientMsg::AddBot if is_leader => self.add_bot(),
            ClientMsg::RemoveBots if is_leader => {
                self.players.retain(|_, p| p.bot.is_none());
                self.roster_dirty = true;
            }
            ClientMsg::NextMap if is_leader && self.phase == Phase::Lobby => {
                self.settings.map = (self.settings.map + 1) % map::MAP_NAMES.len() as u8;
                self.map = Map::by_id(self.settings.map);
                self.roster_dirty = true;
            }
            ClientMsg::Fire { weapon, origin, dirs } => self.handle_fire(id, weapon, origin, dirs),
            ClientMsg::Hit {
                victim,
                weapon,
                headshot,
            } => self.handle_hit(id, victim, weapon, headshot),
            ClientMsg::Grenade { origin, vel } => self.handle_grenade(id, origin, vel),
            _ => {}
        }
    }

    /// A shot or knife swing. Checked against the weapon's fire rate, counted
    /// towards the hits this player may claim, and shown to everyone else.
    fn handle_fire(&mut self, shooter_id: PlayerId, weapon_id: WeaponId, origin: [f32; 3], dirs: Vec<[f32; 3]>) {
        let stats = weapon(weapon_id);
        let origin_v = Vec3::from(origin);
        if self.phase != Phase::Playing
            || weapon_id as usize >= WEAPONS.len()
            || dirs.len() != stats.pellets
            || !origin_v.is_finite()
            || dirs.iter().any(|dir| !Vec3::from(*dir).is_finite())
        {
            return;
        }
        let Some(shooter) = self.players.get_mut(&shooter_id) else {
            return;
        };
        let cost = stats.interval * 0.8;
        if !shooter.alive || shooter.fire_credit < cost || origin_v.distance(shooter.pos) > 6.0 {
            return;
        }
        shooter.fire_credit -= cost;
        if weapon_id == KNIFE {
            shooter.stab_ready = true;
        } else {
            shooter.bullets_out = (shooter.bullets_out + dirs.len() as u32).min(MAX_BULLETS_OUT);
        }
        self.renet.broadcast_message_except(
            shooter_id,
            CH_RELIABLE,
            encode(&ServerMsg::Fire {
                shooter: shooter_id,
                weapon: weapon_id,
                origin,
                dirs,
            }),
        );
    }

    /// The shooter's client says one of its bullets, or its knife, reached
    /// `victim_id`. Accepted if the shooter really had a bullet in the air (or
    /// a knife swing going and the victim within reach).
    fn handle_hit(&mut self, shooter_id: PlayerId, victim_id: PlayerId, weapon_id: WeaponId, headshot: bool) {
        if self.phase != Phase::Playing || shooter_id == victim_id || weapon_id as usize >= WEAPONS.len() {
            return;
        }
        let Some(victim_pos) = self.players.get(&victim_id).filter(|v| v.alive).map(|v| v.pos) else {
            return;
        };
        let Some(shooter) = self.players.get_mut(&shooter_id) else {
            return;
        };
        let allowed = if weapon_id == KNIFE {
            // Generous reach: both players have moved since the swing.
            std::mem::take(&mut shooter.stab_ready)
                && shooter.alive
                && shooter.pos.distance(victim_pos) < KNIFE_RANGE + 3.0
        } else if shooter.bullets_out > 0 {
            // A bullet may land after its shooter has died.
            shooter.bullets_out -= 1;
            true
        } else {
            false
        };
        if allowed {
            let headshot = headshot && weapon_id != KNIFE;
            self.damage(victim_id, shooter_id, hit_damage(weapon_id, headshot), weapon_id);
        }
    }

    fn handle_grenade(&mut self, thrower_id: PlayerId, origin: [f32; 3], vel: [f32; 3]) {
        let (pos, vel) = (Vec3::from(origin), Vec3::from(vel));
        if self.phase != Phase::Playing || !pos.is_finite() || !vel.is_finite() {
            return;
        }
        let Some(thrower) = self.players.get_mut(&thrower_id) else {
            return;
        };
        // A little slack on the recharge for network jitter.
        if !thrower.alive || thrower.grenades < 0.9 || pos.distance(thrower.pos) > 6.0 {
            return;
        }
        thrower.grenades = (thrower.grenades - 1.0).max(0.0);
        let vel = vel.clamp_length_max(70.0);
        self.next_grenade_id += 1;
        let id = self.next_grenade_id;
        self.grenades.push(Grenade {
            id,
            owner: thrower_id,
            pos,
            vel,
            fuse: GRENADE_FUSE,
        });
        self.broadcast(&ServerMsg::Grenade {
            id,
            pos: pos.to_array(),
            vel: vel.to_array(),
        });
    }

    fn update_grenades(&mut self, dt: f32) {
        self.grenade_clock += dt;
        while self.grenade_clock >= GRENADE_STEP {
            self.grenade_clock -= GRENADE_STEP;
            for grenade in &mut self.grenades {
                self.map.step_grenade(&mut grenade.pos, &mut grenade.vel, GRENADE_STEP);
                grenade.fuse -= GRENADE_STEP;
            }
            let (exploding, flying) = std::mem::take(&mut self.grenades)
                .into_iter()
                .partition(|grenade| grenade.fuse <= 0.0);
            self.grenades = flying;
            for grenade in exploding {
                self.explode(grenade);
            }
        }
    }

    /// Hurts everyone in range who is not behind cover, the thrower included.
    fn explode(&mut self, grenade: Grenade) {
        self.broadcast(&ServerMsg::Explosion {
            id: grenade.id,
            pos: grenade.pos.to_array(),
        });
        let center = grenade.pos + Vec3::Y * 0.1;
        let mut hurt = Vec::new();
        for (id, player) in &self.players {
            if !player.alive {
                continue;
            }
            let half = map::player_half(player.flags & FLAG_CROUCH != 0);
            // Distance to the nearest part of their body.
            let distance = center.distance(center.clamp(player.pos - half, player.pos + half));
            let amount = (GRENADE_DAMAGE * (1.0 - distance / GRENADE_RADIUS)).round() as i32;
            let head = player.pos + Vec3::Y * (half.y - 0.15);
            let exposed = self.map.line_of_sight(center, player.pos) || self.map.line_of_sight(center, head);
            if amount > 0 && exposed {
                hurt.push((*id, amount));
            }
        }
        for (id, amount) in hurt {
            // A kill can end the match, and then nobody else gets hurt.
            if self.phase == Phase::Playing {
                self.damage(id, grenade.owner, amount, weapons::GRENADE);
            }
        }
    }

    fn damage(&mut self, victim_id: PlayerId, attacker_id: PlayerId, amount: i32, weapon_id: WeaponId) {
        let mode = self.settings.mode;
        let attacker_team = self.players.get(&attacker_id).map(|attacker| attacker.team);
        let Some(victim) = self.players.get_mut(&victim_id) else {
            return;
        };
        // No friendly fire. Your own grenade still hurts you.
        if is_team_mode(mode) && attacker_id != victim_id && attacker_team == Some(victim.team) {
            return;
        }
        victim.since_damage = 0.0;
        victim.regen = 0.0;
        victim.health = (victim.health - amount).max(0);
        let health = victim.health;
        if health == 0 {
            victim.alive = false;
            victim.deaths += 1;
            victim.respawn_in = RESPAWN_DELAY;
        }
        self.broadcast(&ServerMsg::Hit {
            victim: victim_id,
            attacker: attacker_id,
            health,
        });
        if health > 0 {
            return;
        }

        // Blowing yourself up does not score.
        let mut scorer = None;
        if attacker_id != victim_id
            && let Some(attacker) = self.players.get_mut(&attacker_id)
        {
            attacker.kills += 1;
            scorer = Some((attacker.kills, attacker.team as usize & 1));
        }
        self.broadcast(&ServerMsg::Kill {
            killer: attacker_id,
            victim: victim_id,
            weapon: weapon_id,
        });
        self.roster_dirty = true;
        let Some((kills, team)) = scorer else {
            return;
        };
        let limit = score_limit(mode, self.settings.frag_limit);
        match mode {
            MODE_FFA if kills >= limit => self.end_match(Winner::Player(attacker_id)),
            MODE_TEAMS => {
                self.team_scores[team] += 1;
                if self.team_scores[team] >= limit {
                    self.end_match(Winner::Team(team as u8));
                }
            }
            // On the hill, kills only clear the way.
            _ => {}
        }
    }

    fn start_match(&mut self) {
        println!("[server] match started");
        self.phase = Phase::Playing;
        self.time_left = self.settings.match_minutes * 60.0;
        self.grenades.clear();
        self.team_scores = [0, 0];
        self.hill = HILL_EMPTY;
        self.hill_clock = 0.0;
        for player in self.players.values_mut() {
            player.kills = 0;
            player.deaths = 0;
            player.alive = false;
        }
        if is_team_mode(self.settings.mode) {
            self.balance_bots();
        }
        let roster = self.roster_msg();
        self.broadcast(&roster);
        self.broadcast(&ServerMsg::MatchStarted {
            score_limit: score_limit(self.settings.mode, self.settings.frag_limit),
            time_left: self.time_left,
        });
        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        for id in ids {
            self.spawn_player(id);
        }
    }

    /// Moves bots across until the teams differ by at most one player. People
    /// stay on the team they picked.
    fn balance_bots(&mut self) {
        loop {
            let ones = self.players.values().filter(|p| p.team == 1).count();
            let zeros = self.players.len() - ones;
            let (big, small) = if zeros > ones { (0, 1) } else { (1, 0) };
            if zeros.abs_diff(ones) < 2 {
                break;
            }
            let Some(bot) = self.players.values_mut().find(|p| p.bot.is_some() && p.team == big) else {
                break;
            };
            bot.team = small;
        }
    }

    fn end_match(&mut self, winner: Winner) {
        println!("[server] match ended");
        self.phase = Phase::Lobby;
        self.grenades.clear();
        for player in self.players.values_mut() {
            player.alive = false;
        }
        let roster = self.roster_msg();
        self.broadcast(&roster);
        self.broadcast(&ServerMsg::MatchEnded { winner });
        self.roster_dirty = false;
    }

    /// Spawns at one of the points farthest away from every living player (in
    /// the team modes: from every living enemy).
    fn spawn_player(&mut self, id: PlayerId) {
        let team_mode = is_team_mode(self.settings.mode);
        let my_team = self.players.get(&id).map_or(0, |p| p.team);
        let others: Vec<Vec3> = self
            .players
            .iter()
            .filter(|(other, p)| **other != id && p.alive && !(team_mode && p.team == my_team))
            .map(|(_, p)| p.pos)
            .collect();
        let mut ranked: Vec<(f32, usize)> = self
            .map
            .spawns
            .iter()
            .enumerate()
            .map(|(index, spawn)| {
                let nearest = others
                    .iter()
                    .map(|pos| pos.distance(spawn.pos))
                    .fold(f32::INFINITY, f32::min);
                (nearest, index)
            })
            .collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
        let pick = self.random() as usize % ranked.len().min(3);
        let spawn = &self.map.spawns[ranked[pick].1];
        let (pos, yaw) = (spawn.pos, spawn.yaw);

        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        player.alive = true;
        player.health = MAX_HEALTH;
        player.pos = pos;
        player.yaw = yaw;
        player.pitch = 0.0;
        player.flags = 0;
        player.grenades = GRENADE_MAX;
        player.fire_credit = MAX_FIRE_CREDIT;
        player.since_damage = 0.0;
        player.spawn_seq += 1;
        let spawn_seq = player.spawn_seq;
        if let Some(bot) = &mut player.bot {
            bot.vel = Vec3::ZERO;
            bot.retarget_in = 0.0;
        }
        self.broadcast(&ServerMsg::Spawn {
            id,
            pos: pos.to_array(),
            yaw,
            spawn_seq,
        });
    }

    fn add_bot(&mut self) {
        if self.players.len() >= MAX_PLAYERS {
            return;
        }
        let id = self.next_bot_id;
        self.next_bot_id += 1;
        let name = format!("[bot] {}", BOT_NAMES[(id as usize - 1) % BOT_NAMES.len()]);
        let bot = Bot {
            vel: Vec3::ZERO,
            target: Vec3::ZERO,
            retarget_in: 0.0,
        };
        self.join_counter += 1;
        let player = self.new_player(name, Some(bot));
        self.players.insert(id, player);
        self.roster_dirty = true;
        if self.phase == Phase::Playing {
            let roster = self.roster_msg();
            self.broadcast(&roster);
            self.roster_dirty = false;
            self.spawn_player(id);
        }
    }

    /// Bots are moving targets: they run between random points and hop around,
    /// using the same collision code as real players. They do not shoot back.
    fn update_bots(&mut self, dt: f32) {
        const BOT_SPEED: f32 = 7.0;
        let mut rng = self.rng;
        let hill = (self.settings.mode == MODE_HILL).then_some(self.map.hill);
        for player in self.players.values_mut() {
            let Some(bot) = &mut player.bot else {
                continue;
            };
            if !player.alive {
                continue;
            }
            bot.retarget_in -= dt;
            let mut to_target = bot.target - player.pos;
            to_target.y = 0.0;
            if bot.retarget_in <= 0.0 || to_target.length() < 1.5 {
                let x = (next_random(&mut rng) % 5600) as f32 / 100.0 - 28.0;
                let z = (next_random(&mut rng) % 5600) as f32 / 100.0 - 28.0;
                bot.target = match hill {
                    // On the hill half the time, so there is something to fight over.
                    Some(hill) if next_random(&mut rng) % 2 == 0 => hill + Vec3::new(x, 0.0, z) * 0.2,
                    _ => Vec3::new(x, 0.0, z),
                };
                bot.retarget_in = 2.0 + (next_random(&mut rng) % 300) as f32 / 100.0;
                to_target = bot.target - player.pos;
                to_target.y = 0.0;
            }
            let wish = to_target.normalize_or_zero() * BOT_SPEED;
            let blend = (8.0 * dt).min(1.0);
            bot.vel.x += (wish.x - bot.vel.x) * blend;
            bot.vel.z += (wish.z - bot.vel.z) * blend;
            bot.vel.y -= 22.0 * dt;
            let wanted = Vec3::new(bot.vel.x, 0.0, bot.vel.z);
            let grounded = self.map.move_and_slide(&mut player.pos, &mut bot.vel, map::PLAYER_HALF, dt);
            if let Some(pad) = self.map.pad_at(player.pos - Vec3::Y * map::PLAYER_HALF.y) {
                // Bots aim with arrow pads from a standstill so they land where intended.
                bot.vel = match pad.direction() {
                    Some(_) => pad.launch,
                    None => pad.launch_velocity(bot.vel),
                };
            } else if grounded {
                // Hop when blocked by something, and every now and then for fun.
                let blocked = Vec3::new(bot.vel.x, 0.0, bot.vel.z).length() < wanted.length() * 0.5;
                if blocked || next_random(&mut rng) % 90 == 0 {
                    bot.vel.y = 8.0;
                }
            }
            if wish.length_squared() > 0.01 {
                player.yaw = (-wish.x).atan2(-wish.z);
            }
        }
        self.rng = rng;
    }

    fn new_player(&self, name: String, bot: Option<Bot>) -> Player {
        // Pick the least used colour so players are easy to tell apart.
        let color = (0..PLAYER_COLOR_COUNT)
            .min_by_key(|c| self.players.values().filter(|p| p.color == *c).count())
            .unwrap_or(0);
        // And the smaller team.
        let ones = self.players.values().filter(|p| p.team == 1).count();
        let team = (ones * 2 < self.players.len()) as u8;
        Player {
            name,
            color,
            team,
            kills: 0,
            deaths: 0,
            join_order: self.join_counter,
            alive: false,
            health: 0,
            pos: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            flags: 0,
            weapon: 0,
            respawn_in: 0.0,
            spawn_seq: 0,
            fire_credit: 0.0,
            bullets_out: 0,
            stab_ready: false,
            since_damage: 0.0,
            regen: 0.0,
            grenades: 0.0,
            bot,
        }
    }

    /// The lobby leader is the human who has been connected the longest.
    fn leader(&self) -> Option<PlayerId> {
        self.players
            .iter()
            .filter(|(_, p)| p.bot.is_none())
            .min_by_key(|(_, p)| p.join_order)
            .map(|(id, _)| *id)
    }

    fn roster_msg(&self) -> ServerMsg {
        let mut players: Vec<(&PlayerId, &Player)> = self.players.iter().collect();
        players.sort_by_key(|(_, p)| p.join_order);
        ServerMsg::Roster {
            players: players
                .into_iter()
                .map(|(id, p)| PlayerInfo {
                    id: *id,
                    name: p.name.clone(),
                    color: p.color,
                    team: p.team,
                    kills: p.kills,
                    deaths: p.deaths,
                    is_bot: p.bot.is_some(),
                })
                .collect(),
            leader: self.leader().unwrap_or(0),
            map: self.settings.map,
            mode: self.settings.mode,
        }
    }

    fn send(&mut self, id: PlayerId, msg: &ServerMsg) {
        self.renet.send_message(id, CH_RELIABLE, encode(msg));
    }

    fn broadcast(&mut self, msg: &ServerMsg) {
        self.renet.broadcast_message(CH_RELIABLE, encode(msg));
    }

    fn random(&mut self) -> u64 {
        next_random(&mut self.rng)
    }
}

/// Keeps the hosted server alive; dropping it shuts the server down.
pub struct ServerHandle {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// xorshift64
fn next_random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state >> 11
}

fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(16)
        .collect::<String>()
        .trim()
        .to_string();
    if cleaned.is_empty() {
        "Player".to_string()
    } else {
        cleaned
    }
}
