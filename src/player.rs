//! The local player: input, movement, and the first-person camera.
//!
//! Movement is client-authoritative (we simulate ourselves and tell the server
//! where we are), which keeps it perfectly responsive at any ping. That is the
//! right trade-off for private lobbies among friends.

use std::f32::consts::FRAC_PI_2;

use bevy::{
    input::mouse::AccumulatedMouseMotion,
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};

use crate::{
    AppState,
    map::{self, CROUCH_EYE, PLAYER_HALF, STAND_EYE},
    net::{Connection, NetMsg, Session},
    protocol::*,
    settings::{Action, ActionInput, Settings, SettingsMenu},
    weapons::{DRAW_TIME, KNIFE, PISTOL, RIFLE, Weapon, WeaponId, weapon},
    world::{GameMap, MainCamera, ViewModel, ViewModelPart},
};

// --- Movement tuning (metres, seconds). Tweak these to change the feel. ---
const RUN_SPEED: f32 = 9.5;
const GROUND_ACCEL: f32 = 10.0;
const FRICTION: f32 = 6.0;
const STOP_SPEED: f32 = 3.0;
/// Quake-style air strafing: in the air you can only add up to this much speed
/// in the wished direction per frame, which is what makes strafe-jumping gain speed.
const AIR_WISH_SPEED: f32 = 1.0;
const AIR_ACCEL: f32 = 10.0;
/// Extra air control (m/s²) that only applies below running speed.
const AIR_CONTROL_ACCEL: f32 = 25.0;
const GRAVITY: f32 = 22.0;
const JUMP_SPEED: f32 = 8.0;
const AIR_JUMPS: u32 = 1;
const DASH_SPEED: f32 = 20.0;
pub const DASH_COOLDOWN: f32 = 1.5;
const MAX_SPEED: f32 = 45.0;
const CROUCH_SPEED: f32 = 4.5;
/// Walking speed while aiming down sights.
const AIM_SPEED: f32 = 6.0;
/// Crouching on the ground at this speed or more starts a slide.
const SLIDE_MIN_SPEED: f32 = 7.0;
/// A slide tops your speed up to this, if `SLIDE_BOOST_COOLDOWN` has passed.
const SLIDE_BOOST_SPEED: f32 = 15.0;
const SLIDE_BOOST_COOLDOWN: f32 = 2.0;
/// Speed lost per second while sliding (m/s²). Running friction is far higher.
const SLIDE_DECEL: f32 = 6.0;
/// Below this speed a slide turns into an ordinary crouch.
const SLIDE_END_SPEED: f32 = 5.0;

/// Looking straight up or down stops just short of vertical.
const PITCH_LIMIT: f32 = FRAC_PI_2 - 0.01;
/// Recoil: the view holds where shots threw it for this long after the last
/// one, then drifts back at this rate (fraction per second, roughly).
const KICK_HOLD: f32 = 0.14;
const KICK_RECOVERY: f32 = 9.0;

/// Field of view of the menu camera, in radians.
pub const BASE_FOV: f32 = 1.3;
pub const VIEW_MODEL_REST: Vec3 = Vec3::new(0.2, -0.17, -0.42);
/// `LocalPlayer::slot` when the knife is out (0 and 1 are the gun slots).
pub const KNIFE_SLOT: usize = 2;

/// One of the two guns carried.
#[derive(Clone, Copy)]
pub struct GunSlot {
    pub weapon: WeaponId,
    pub ammo: u32,
    /// Seconds until it can fire again.
    pub cooldown: f32,
}

impl GunSlot {
    fn full(weapon_id: WeaponId) -> GunSlot {
        GunSlot {
            weapon: weapon_id,
            ammo: weapon(weapon_id).magazine,
            cooldown: 0.0,
        }
    }
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalPlayer>()
            .init_resource::<Controls>()
            .add_systems(OnEnter(AppState::Playing), capture_mouse)
            .add_systems(OnExit(AppState::Playing), hide_view_model)
            .add_systems(
                Update,
                (
                    handle_net.run_if(resource_exists::<Connection>),
                    (toggle_pause, look, movement)
                        .chain()
                        .in_set(PlayerSystems::Simulate)
                        .run_if(in_state(AppState::Playing)),
                    follow_camera
                        .in_set(PlayerSystems::Camera)
                        .run_if(in_state(AppState::Playing)),
                    orbit_camera.run_if(not(in_state(AppState::Playing))),
                    grab_cursor,
                ),
            )
            .configure_sets(
                Update,
                (PlayerSystems::Simulate, PlayerSystems::Camera).chain(),
            )
            .add_systems(
                FixedUpdate,
                send_state
                    .run_if(in_state(AppState::Playing))
                    .run_if(resource_exists::<Connection>),
            );
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlayerSystems {
    /// Input and movement.
    Simulate,
    /// The camera is moved to the player's eyes.
    Camera,
}

#[derive(Resource)]
pub struct LocalPlayer {
    /// Centre of the collision box.
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub grounded: bool,
    pub air_jumps: u32,
    pub dash_cooldown: f32,
    pub alive: bool,
    pub health: i32,
    pub spawn_seq: u32,
    pub guns: [GunSlot; 2],
    /// What is in hand: a gun slot (0 or 1) or `KNIFE_SLOT`.
    pub slot: usize,
    /// Seconds until the weapon just switched to is up and usable.
    pub draw: f32,
    pub knife_cooldown: f32,
    /// 1 right after a shot or stab, decays to 0. Drives the gun kick.
    pub recoil: f32,
    /// How far shots have thrown the view (pitch, yaw) that has not yet
    /// drifted back. Pulling the mouse against it pays it off early.
    pub view_kick: Vec2,
    /// Seconds after the last shot before the view starts drifting back.
    pub kick_hold: f32,
    pub killed_by: Option<String>,
    /// Seconds the hit marker stays visible.
    pub hit_marker: f32,
    /// Seconds the hit marker stays yellow, for a headshot.
    pub headshot_marker: f32,
    /// 1 right after a kill, decays to 0. Turns the hit marker red.
    pub kill_marker: f32,
    /// 1 right after taking damage, decays to 0.
    pub damage_flash: f32,
    /// Crouching or sliding: the collision box is the short one.
    pub crouching: bool,
    pub sliding: bool,
    slide_boost_cooldown: f32,
    /// Eye height above the feet, eased between standing and crouching.
    view_height: f32,
    /// 0 = hip, 1 = fully aimed down sights.
    pub aim: f32,
    /// Seconds until the reload finishes; 0 when not reloading.
    pub reload: f32,
    /// Grenades in hand (whole ones can be thrown; the fraction is the recharge).
    pub grenades: f32,
    /// 1 right after throwing a grenade, decays to 0. Drives the gun dip.
    pub throw_anim: f32,
}

impl Default for LocalPlayer {
    fn default() -> Self {
        Self {
            pos: Vec3::new(0.0, 10.0, 0.0),
            vel: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            grounded: false,
            air_jumps: AIR_JUMPS,
            dash_cooldown: 0.0,
            alive: false,
            health: 0,
            spawn_seq: 0,
            guns: [GunSlot::full(RIFLE), GunSlot::full(PISTOL)],
            slot: 0,
            draw: 0.0,
            knife_cooldown: 0.0,
            recoil: 0.0,
            view_kick: Vec2::ZERO,
            kick_hold: 0.0,
            killed_by: None,
            hit_marker: 0.0,
            headshot_marker: 0.0,
            kill_marker: 0.0,
            damage_flash: 0.0,
            crouching: false,
            sliding: false,
            slide_boost_cooldown: 0.0,
            view_height: STAND_EYE,
            aim: 0.0,
            reload: 0.0,
            grenades: GRENADE_MAX,
            throw_anim: 0.0,
        }
    }
}

impl LocalPlayer {
    /// Half extents of the collision box right now.
    pub fn half(&self) -> Vec3 {
        map::player_half(self.crouching)
    }

    pub fn feet(&self) -> Vec3 {
        self.pos - Vec3::Y * self.half().y
    }

    pub fn eye(&self) -> Vec3 {
        self.feet() + Vec3::Y * self.view_height
    }

    pub fn reloading(&self) -> bool {
        self.reload > 0.0
    }

    /// The weapon in hand.
    pub fn weapon_id(&self) -> WeaponId {
        self.guns.get(self.slot).map_or(KNIFE, |gun| gun.weapon)
    }

    pub fn weapon(&self) -> &'static Weapon {
        weapon(self.weapon_id())
    }

    pub fn horizontal_speed(&self) -> f32 {
        self.vel.with_y(0.0).length()
    }

    /// Throws the view by `kick` (pitch, yaw) for a shot just fired.
    pub fn kick_view(&mut self, kick: Vec2) {
        let pitch = (self.pitch + kick.x).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        self.view_kick += Vec2::new(pitch - self.pitch, kick.y);
        self.pitch = pitch;
        self.yaw += kick.y;
        self.kick_hold = KICK_HOLD;
    }
}

/// Mouse movement against the recoil counts towards bringing the view back.
fn pay_off(owed: &mut f32, moved: f32) {
    if *owed * moved < 0.0 {
        *owed = owed.signum() * (owed.abs() - moved.abs()).max(0.0);
    }
}

#[derive(Resource, Default)]
pub struct Controls {
    /// True while the mouse is grabbed and input drives the player. False = paused.
    pub captured: bool,
}

fn capture_mouse(mut controls: ResMut<Controls>) {
    controls.captured = true;
}

fn hide_view_model(mut view_model: Single<&mut Visibility, With<ViewModel>>) {
    **view_model = Visibility::Hidden;
}

fn handle_net(
    mut messages: MessageReader<NetMsg>,
    session: Res<Session>,
    settings: Res<Settings>,
    mut player: ResMut<LocalPlayer>,
) {
    for NetMsg(msg) in messages.read() {
        match msg {
            ServerMsg::MatchStarted { .. } => *player = LocalPlayer::default(),
            ServerMsg::Spawn { id, pos, yaw, spawn_seq } if *id == session.my_id => {
                *player = LocalPlayer {
                    pos: Vec3::from(*pos),
                    yaw: *yaw,
                    alive: true,
                    health: MAX_HEALTH,
                    spawn_seq: *spawn_seq,
                    // Every life starts with the loadout picked in the menus.
                    guns: [GunSlot::full(settings.primary), GunSlot::full(settings.secondary)],
                    ..default()
                };
            }
            ServerMsg::Hit { victim, health, .. } if *victim == session.my_id => {
                player.health = *health;
                player.damage_flash = 1.0;
            }
            // Grenade damage is decided by the server, so this is where we learn of it.
            ServerMsg::Hit { attacker, .. } if *attacker == session.my_id => player.hit_marker = 0.15,
            ServerMsg::Kill { killer, victim, .. } if *victim == session.my_id => {
                player.alive = false;
                player.health = 0;
                player.killed_by = Some(session.name_of(*killer).to_string());
            }
            _ => {}
        }
    }
}

/// Escape pauses and resumes; with the settings panel open it backs out of that instead.
fn toggle_pause(keys: Res<ButtonInput<KeyCode>>, mut controls: ResMut<Controls>, mut menu: ResMut<SettingsMenu>) {
    if keys.just_pressed(KeyCode::Escape) {
        if menu.open {
            menu.back();
        } else {
            controls.captured = !controls.captured;
        }
    }
}

fn look(
    mouse: Res<AccumulatedMouseMotion>,
    controls: Res<Controls>,
    settings: Res<Settings>,
    mut player: ResMut<LocalPlayer>,
) {
    if !controls.captured || mouse.delta == Vec2::ZERO {
        return;
    }
    // Aiming turns the view slower in step with the zoom, so it feels the same on screen.
    let aimed = player.weapon().zoom * settings.ads_sensitivity;
    let sensitivity = settings.sensitivity * 0.001 * (1.0 + (aimed - 1.0) * player.aim);
    let p = &mut *player;
    let turn = -mouse.delta.x * sensitivity;
    let pitch = (p.pitch - mouse.delta.y * sensitivity).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    pay_off(&mut p.view_kick.x, pitch - p.pitch);
    pay_off(&mut p.view_kick.y, turn);
    p.yaw += turn;
    p.pitch = pitch;
}

fn movement(time: Res<Time>, input: ActionInput, map: Res<GameMap>, mut player: ResMut<LocalPlayer>) {
    let dt = time.delta_secs().min(0.05);
    let p = &mut *player;
    p.dash_cooldown = (p.dash_cooldown - dt).max(0.0);
    p.slide_boost_cooldown = (p.slide_boost_cooldown - dt).max(0.0);
    if !p.alive {
        return;
    }

    // Wished direction on the ground plane, from the keys held.
    let forward = map::look_dir(p.yaw, 0.0);
    let right = Vec3::new(-forward.z, 0.0, forward.x);
    let mut wish = Vec3::ZERO;
    if input.pressed(Action::Forward) {
        wish += forward;
    }
    if input.pressed(Action::Back) {
        wish -= forward;
    }
    if input.pressed(Action::Right) {
        wish += right;
    }
    if input.pressed(Action::Left) {
        wish -= right;
    }
    let wish = wish.normalize_or_zero();

    // Crouching shrinks the box towards the feet; standing up needs headroom.
    let crouch_drop = PLAYER_HALF.y - map::CROUCH_HALF.y;
    if input.pressed(Action::Crouch) {
        if !p.crouching {
            p.crouching = true;
            p.pos.y -= crouch_drop;
            if p.grounded {
                start_slide(p);
            }
        }
    } else if p.crouching {
        let standing = p.pos + Vec3::Y * crouch_drop;
        if map.0.is_free(standing, PLAYER_HALF) {
            p.crouching = false;
            p.sliding = false;
            p.pos = standing;
        }
    }

    let aiming = input.pressed(Action::Aim) && !p.reloading() && p.slot != KNIFE_SLOT;
    p.aim += (aiming as u8 as f32 - p.aim) * (dt * 14.0).min(1.0);

    if p.grounded {
        p.air_jumps = AIR_JUMPS;
        // Holding jump hops again the moment you land, skipping ground friction:
        // that is what lets you keep (and build) speed by bunny hopping.
        if input.pressed(Action::Jump) {
            p.vel.y = JUMP_SPEED;
            p.grounded = false;
            p.sliding = false;
            air_accelerate(&mut p.vel, wish, dt);
        } else if p.sliding {
            // A slide keeps its direction and bleeds speed slowly.
            let speed = p.horizontal_speed();
            let slowed = (speed - SLIDE_DECEL * dt).max(0.0);
            if slowed < SLIDE_END_SPEED {
                p.sliding = false;
            } else {
                p.vel.x *= slowed / speed;
                p.vel.z *= slowed / speed;
            }
        } else {
            let top_speed = if p.crouching {
                CROUCH_SPEED
            } else if aiming {
                AIM_SPEED
            } else {
                RUN_SPEED * p.weapon().mobility
            };
            apply_friction(&mut p.vel, dt);
            accelerate(&mut p.vel, wish, top_speed, GROUND_ACCEL * top_speed, dt);
        }
    } else {
        if input.just_pressed(Action::Jump) && p.air_jumps > 0 {
            p.air_jumps -= 1;
            p.vel.y = JUMP_SPEED;
        }
        air_accelerate(&mut p.vel, wish, dt);
    }

    // Dash: redirect all momentum where you are steering, with a minimum speed.
    if input.just_pressed(Action::Dash) && p.dash_cooldown <= 0.0 {
        p.dash_cooldown = DASH_COOLDOWN;
        let dir = if wish == Vec3::ZERO { forward } else { wish };
        let speed = p.vel.with_y(0.0).length().max(DASH_SPEED);
        p.vel = Vec3::new(dir.x * speed, p.vel.y.max(0.0), dir.z * speed);
    }

    p.vel.y -= GRAVITY * dt;
    let horizontal = p.vel.with_y(0.0).clamp_length_max(MAX_SPEED);
    p.vel = Vec3::new(horizontal.x, p.vel.y.max(-60.0), horizontal.z);

    let was_grounded = p.grounded;
    let half = p.half();
    p.grounded = map.0.move_and_slide(&mut p.pos, &mut p.vel, half, dt);
    // Landing with crouch held carries the speed into a slide.
    if p.grounded && !was_grounded && p.crouching {
        start_slide(p);
    }

    if let Some(pad) = map.0.pad_at(p.feet()) {
        p.vel = pad.launch_velocity(p.vel);
        p.grounded = false;
        p.sliding = false;
    }
    // Safety net; the arena is sealed, so this should never trigger.
    if p.pos.y < -20.0 || !p.pos.is_finite() {
        p.pos = Vec3::new(0.0, 8.0, 0.0);
        p.vel = Vec3::ZERO;
    }
}

/// Called when a crouching player touches the ground: fast enough, and it is a slide.
fn start_slide(p: &mut LocalPlayer) {
    let speed = p.horizontal_speed();
    if p.sliding || speed < SLIDE_MIN_SPEED {
        return;
    }
    p.sliding = true;
    if p.slide_boost_cooldown <= 0.0 && speed < SLIDE_BOOST_SPEED {
        p.slide_boost_cooldown = SLIDE_BOOST_COOLDOWN;
        p.vel.x *= SLIDE_BOOST_SPEED / speed;
        p.vel.z *= SLIDE_BOOST_SPEED / speed;
    }
}

/// Source/Quake acceleration: speed up along `wish_dir` until the velocity's
/// projection onto it reaches `wish_speed`.
fn accelerate(vel: &mut Vec3, wish_dir: Vec3, wish_speed: f32, accel: f32, dt: f32) {
    let add = wish_speed - vel.dot(wish_dir);
    if add > 0.0 {
        *vel += wish_dir * (accel * dt).min(add);
    }
}

fn air_accelerate(vel: &mut Vec3, wish_dir: Vec3, dt: f32) {
    // Below running speed you get plain air control, so hopping from a standstill
    // is not a crawl. Above it, only strafing adds speed.
    if vel.with_y(0.0).length() < RUN_SPEED {
        accelerate(vel, wish_dir, RUN_SPEED, AIR_CONTROL_ACCEL, dt);
    }
    accelerate(vel, wish_dir, AIR_WISH_SPEED, AIR_ACCEL * RUN_SPEED, dt);
}

fn apply_friction(vel: &mut Vec3, dt: f32) {
    let speed = vel.with_y(0.0).length();
    if speed < 0.01 {
        vel.x = 0.0;
        vel.z = 0.0;
        return;
    }
    let drop = speed.max(STOP_SPEED) * FRICTION * dt;
    let scale = (speed - drop).max(0.0) / speed;
    vel.x *= scale;
    vel.z *= scale;
}

fn follow_camera(
    time: Res<Time>,
    settings: Res<Settings>,
    mut player: ResMut<LocalPlayer>,
    camera: Single<(&mut Transform, &mut Projection), With<MainCamera>>,
    view_model: Single<(&mut Transform, &mut Visibility), (With<ViewModel>, Without<MainCamera>, Without<ViewModelPart>)>,
    mut parts: Query<
        (&ViewModelPart, &mut Transform, &mut Visibility),
        (Without<MainCamera>, Without<ViewModel>),
    >,
) {
    let dt = time.delta_secs();
    let knife = player.slot == KNIFE_SLOT;
    // A stab lasts longer than a gun's kick.
    player.recoil = (player.recoil - dt * if knife { 4.0 } else { 9.0 }).max(0.0);
    player.throw_anim = (player.throw_anim - dt * 2.5).max(0.0);
    // Once the shooting stops, the view drifts back to where it was aimed.
    player.kick_hold = (player.kick_hold - dt).max(0.0);
    if player.kick_hold <= 0.0 && player.view_kick != Vec2::ZERO {
        let back = player.view_kick * (dt * KICK_RECOVERY).min(1.0);
        player.view_kick -= back;
        player.pitch = (player.pitch - back.x).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        player.yaw -= back.y;
        if player.view_kick.length_squared() < 1e-8 {
            player.view_kick = Vec2::ZERO;
        }
    }
    player.hit_marker = (player.hit_marker - dt).max(0.0);
    player.headshot_marker = (player.headshot_marker - dt).max(0.0);
    player.kill_marker = (player.kill_marker - dt * 1.6).max(0.0);
    player.damage_flash = (player.damage_flash - dt * 2.5).max(0.0);
    let eye_target = if player.crouching { CROUCH_EYE } else { STAND_EYE };
    player.view_height += (eye_target - player.view_height) * (dt * 14.0).min(1.0);

    let (mut transform, mut projection) = camera.into_inner();
    // When dead the camera stays where we died, but you can still look around.
    if player.alive {
        transform.translation = player.eye();
    }
    transform.rotation = Quat::from_euler(EulerRot::YXZ, player.yaw, player.pitch, 0.0);

    // Widen the field of view a little at speed; narrow it when aiming.
    if let Projection::Perspective(perspective) = &mut *projection {
        let boost = ((player.horizontal_speed() - RUN_SPEED) / 25.0).clamp(0.0, 1.0) * 0.22;
        let hip = settings.fov.to_radians() + boost;
        let target = hip * (1.0 + (player.weapon().zoom - 1.0) * player.aim);
        perspective.fov += (target - perspective.fov) * (dt * 14.0).min(1.0);
    }

    // The gun: slides to the middle when aiming, kicks when fired, dips for a
    // grenade throw, and tips over while the magazine is swapped.
    let smooth = |t: f32| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let stats = player.weapon();
    let [width, height, length] = stats.size;
    let reload = if player.reloading() {
        1.0 - player.reload / stats.reload
    } else {
        0.0
    };
    // 0 -> 1 as the gun tips over, 1 -> 0 as it comes back up.
    let tipped = smooth(reload / 0.18) * smooth((1.0 - reload) / 0.18);
    // 0 -> 1 as the old magazine drops away, 1 -> 0 as the new one goes in.
    let magazine_out = smooth((reload - 0.15) / 0.2) * smooth((0.8 - reload) / 0.2);
    let throw = (player.throw_anim * std::f32::consts::PI).sin();

    // 1 -> 0 as a weapon just switched to comes up from below.
    let drawing = smooth(player.draw / DRAW_TIME);
    // A gun kicks back; the knife lunges forward and across.
    let (kick, kick_pitch, kick_yaw) = if knife {
        let lunge = (player.recoil * std::f32::consts::PI).sin();
        (Vec3::new(-0.14, 0.05, -0.22) * lunge, -0.3 * lunge, 0.7 * lunge)
    } else {
        (
            Vec3::new(0.0, 0.01, 0.07) * player.recoil * (1.0 - 0.6 * player.aim),
            player.recoil * 0.07,
            0.0,
        )
    };
    // Aimed: centred, with the front sight just under the crosshair.
    let aimed = Vec3::new(0.0, -(height / 2.0 + 0.048), -0.31 - length / 2.0);

    let (mut gun, mut gun_visibility) = view_model.into_inner();
    gun.translation = VIEW_MODEL_REST.lerp(aimed, player.aim)
        + kick
        + Vec3::new(0.0, -0.06, 0.0) * tipped
        + Vec3::new(0.0, -0.16, 0.05) * throw
        + Vec3::new(0.0, -0.3, 0.0) * drawing;
    gun.rotation = Quat::from_rotation_x(kick_pitch + tipped * 0.45 - throw * 0.5 - drawing * 0.9)
        * Quat::from_rotation_y(kick_yaw)
        * Quat::from_rotation_z(tipped * 0.6);

    // The same few boxes make every weapon; only their sizes differ.
    let rear = length / 2.0;
    for (part, mut transform, mut visibility) in &mut parts {
        let (at, size, shown) = match part {
            ViewModelPart::Body => (Vec3::ZERO, Vec3::new(width, height, length), true),
            // The knife's handle sits behind the blade.
            ViewModelPart::Grip if knife => (
                Vec3::new(0.0, 0.0, rear + 0.05),
                Vec3::new(0.026, 0.03, 0.1),
                true,
            ),
            ViewModelPart::Grip => (
                Vec3::new(0.0, -height / 2.0 - 0.035, rear - 0.04),
                Vec3::new(width * 0.8, 0.08, 0.04),
                true,
            ),
            ViewModelPart::Sight => (
                Vec3::new(0.0, height / 2.0 + 0.009, 0.01 - rear),
                Vec3::new(0.006, 0.018, 0.006),
                !knife,
            ),
            ViewModelPart::Magazine => (
                Vec3::new(0.0, -height / 2.0 - 0.045, (rear - 0.13).max(rear - 0.045 - length * 0.3))
                    + Vec3::new(0.0, -0.22, 0.0) * magazine_out,
                Vec3::new(width * 0.7, 0.1, 0.05),
                !knife,
            ),
        };
        transform.translation = at;
        transform.scale = size;
        let wanted = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
    let wanted = if player.alive {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if *gun_visibility != wanted {
        *gun_visibility = wanted;
    }
}

/// Slow fly-around of the arena behind the menus.
fn orbit_camera(
    time: Res<Time>,
    camera: Single<(&mut Transform, &mut Projection), With<MainCamera>>,
) {
    let (mut transform, mut projection) = camera.into_inner();
    let angle = time.elapsed_secs() * 0.12;
    *transform = Transform::from_xyz(angle.cos() * 46.0, 26.0, angle.sin() * 46.0)
        .looking_at(Vec3::new(0.0, 2.0, 0.0), Vec3::Y);
    if let Projection::Perspective(perspective) = &mut *projection
        && perspective.fov != BASE_FOV
    {
        perspective.fov = BASE_FOV;
    }
}

fn grab_cursor(
    state: Res<State<AppState>>,
    controls: Res<Controls>,
    args: Res<crate::LaunchArgs>,
    mut cursor: Single<&mut CursorOptions>,
) {
    // The scripted self-test must not steal the developer's mouse.
    let grab = *state.get() == AppState::Playing && controls.captured && args.selftest.is_none();
    let mode = if grab {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
    if cursor.grab_mode != mode || cursor.visible == grab {
        cursor.grab_mode = mode;
        cursor.visible = !grab;
    }
}

fn send_state(player: Res<LocalPlayer>, mut connection: ResMut<Connection>) {
    if player.alive {
        connection.send_state(&ClientState {
            pos: player.pos.to_array(),
            yaw: player.yaw,
            pitch: player.pitch,
            flags: (player.crouching as u8 * FLAG_CROUCH) | (player.reloading() as u8 * FLAG_RELOAD),
            weapon: player.weapon_id(),
            spawn_seq: player.spawn_seq,
        });
    }
}
