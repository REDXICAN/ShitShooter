//! Other players: spawned from server snapshots and smoothly interpolated.

use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;

use crate::{
    AppState,
    map::{CROUCH_HALF, PLAYER_HALF},
    net::{NetMsg, Session, SnapshotMsg},
    player::LocalPlayer,
    gun_model::{FittedModels, GunHolder, GunModelSystems},
    protocol::*,
    weapons::{WeaponId, weapon},
    world::SharedAssets,
};

/// Remote players are drawn this far in the past, so there are always two
/// snapshots to blend between (snapshots arrive every ~33 ms).
const INTERP_DELAY: f64 = 0.075;

// The model, measured from the centre of the standing collision box (feet at
// -0.9, top of the head at +0.9). Crouching squashes the body and lowers the head.
const BODY_HEIGHT: f32 = 1.3;
const HEAD_RADIUS: f32 = 0.25;
/// The head sinks a little into the shoulders.
const NECK: f32 = HEAD_RADIUS * 0.8;
const HEAD_Y: f32 = BODY_HEIGHT - PLAYER_HALF.y + NECK;

pub struct RemotePlugin;

impl Plugin for RemotePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RemoteIndex>()
            .add_systems(OnExit(AppState::Playing), clear_index)
            .add_systems(
                Update,
                (apply_snapshots, reset_on_spawn, interpolate)
                    .chain()
                    .in_set(RemoteSystems)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                size_gun_boxes
                    .after(GunModelSystems::Show)
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemoteSystems;

#[derive(Component)]
pub struct RemotePlayer {
    pub id: PlayerId,
    pub alive: bool,
    /// Their collision box is the short one (crouching or sliding).
    pub crouching: bool,
    reloading: bool,
    /// The weapon in their hands.
    weapon: WeaponId,
    pitch: f32,
    /// 0 = standing, 1 = crouched; eased so the model does not snap.
    crouch_pose: f32,
    /// 0 = gun up, 1 = gun lowered for a reload.
    reload_pose: f32,
    samples: VecDeque<Sample>,
}

struct Sample {
    time: f64,
    pos: Vec3,
    yaw: f32,
    pitch: f32,
}

/// The part of a remote player that tilts up and down with their aim: head and gun.
#[derive(Component)]
struct HeadPivot;

#[derive(Component)]
struct Body;

/// Holds the gun: its model, or `GunBox` for weapons without one.
#[derive(Component)]
struct Gun;

#[derive(Component)]
struct GunBox;

#[derive(Resource, Default)]
struct RemoteIndex(HashMap<PlayerId, Entity>);

fn clear_index(mut index: ResMut<RemoteIndex>) {
    index.0.clear();
}

fn apply_snapshots(
    mut commands: Commands,
    mut snapshots: MessageReader<SnapshotMsg>,
    time: Res<Time<Real>>,
    session: Res<Session>,
    assets: Res<SharedAssets>,
    mut index: ResMut<RemoteIndex>,
    mut local: ResMut<LocalPlayer>,
    mut remotes: Query<(&mut RemotePlayer, &mut Visibility)>,
) {
    let Some(SnapshotMsg(snapshot)) = snapshots.read().last() else {
        return;
    };
    let now = time.elapsed_secs_f64();

    for snap in &snapshot.players {
        if snap.id == session.my_id {
            // The server owns our health and armor.
            if local.alive && local.health != snap.health {
                local.health = snap.health;
            }
            if local.alive && local.armor != snap.armor {
                local.armor = snap.armor;
            }
            continue;
        }
        let sample = Sample {
            time: now,
            pos: Vec3::from(snap.pos),
            yaw: snap.yaw,
            pitch: snap.pitch,
        };
        if let Some((mut remote, mut visibility)) = index
            .0
            .get(&snap.id)
            .and_then(|entity| remotes.get_mut(*entity).ok())
        {
            if !snap.alive {
                remote.samples.clear();
            } else {
                remote.samples.push_back(sample);
                if remote.samples.len() > 16 {
                    remote.samples.pop_front();
                }
            }
            remote.alive = snap.alive;
            remote.crouching = snap.flags & FLAG_CROUCH != 0;
            remote.reloading = snap.flags & FLAG_RELOAD != 0;
            remote.weapon = snap.weapon;
            let wanted = if snap.alive {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *visibility != wanted {
                *visibility = wanted;
            }
        } else if !index.0.contains_key(&snap.id)
            && let Some(info) = session.roster.iter().find(|p| p.id == snap.id)
        {
            // Team colours in the team modes (the first two are red and blue).
            let color = if session.team_mode() { info.team & 1 } else { info.color };
            let material = assets.player_colors[color as usize % assets.player_colors.len()].clone();
            let entity = commands
                .spawn((
                    RemotePlayer {
                        id: snap.id,
                        alive: snap.alive,
                        crouching: snap.flags & FLAG_CROUCH != 0,
                        reloading: snap.flags & FLAG_RELOAD != 0,
                        weapon: snap.weapon,
                        pitch: snap.pitch,
                        crouch_pose: 0.0,
                        reload_pose: 0.0,
                        samples: VecDeque::from([sample]),
                    },
                    Transform::from_translation(Vec3::from(snap.pos)),
                    if snap.alive {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    },
                    DespawnOnExit(AppState::Playing),
                    children![
                        (
                            Body,
                            Mesh3d(assets.body.clone()),
                            MeshMaterial3d(material.clone()),
                            Transform::from_xyz(0.0, BODY_HEIGHT / 2.0 - PLAYER_HALF.y, 0.0),
                        ),
                        (
                            HeadPivot,
                            Transform::from_xyz(0.0, HEAD_Y, 0.0),
                            Visibility::default(),
                            children![
                                (
                                    Mesh3d(assets.sphere.clone()),
                                    MeshMaterial3d(material),
                                    Transform::from_scale(Vec3::splat(HEAD_RADIUS * 2.0)),
                                ),
                                // Visor: shows which way they are facing.
                                (
                                    Mesh3d(assets.cube.clone()),
                                    MeshMaterial3d(assets.dark.clone()),
                                    Transform::from_xyz(0.0, 0.03, -0.17)
                                        .with_scale(Vec3::new(0.36, 0.11, 0.2)),
                                ),
                                (
                                    Gun,
                                    GunHolder { weapon: snap.weapon, shadows: true },
                                    Transform::from_translation(GUN_REST)
                                        .with_scale(Vec3::splat(REMOTE_GUN_SCALE)),
                                    Visibility::default(),
                                    children![(
                                        GunBox,
                                        Mesh3d(assets.cube.clone()),
                                        MeshMaterial3d(assets.dark.clone()),
                                        Transform::default(),
                                        Visibility::Hidden,
                                    )],
                                ),
                            ],
                        ),
                    ],
                ))
                .id();
            index.0.insert(snap.id, entity);
        }
    }

    // Players that left.
    index.0.retain(|id, entity| {
        let present = snapshot.players.iter().any(|snap| snap.id == *id);
        if !present {
            commands.entity(*entity).despawn();
        }
        present
    });
}

/// A respawn is a teleport: do not blend from the place they died.
fn reset_on_spawn(
    mut messages: MessageReader<NetMsg>,
    index: Res<RemoteIndex>,
    mut remotes: Query<&mut RemotePlayer>,
) {
    for NetMsg(msg) in messages.read() {
        if let ServerMsg::Spawn { id, .. } = msg
            && let Some(mut remote) = index.0.get(id).and_then(|e| remotes.get_mut(*e).ok())
        {
            remote.samples.clear();
        }
    }
}

/// Where the gun is held, relative to the head.
const GUN_REST: Vec3 = Vec3::new(0.3, -0.3, -0.45);
/// Twice the size it has in first person, so it reads at a distance.
const REMOTE_GUN_SCALE: f32 = 2.1;

type Part<T> = (With<T>, Without<RemotePlayer>);

fn interpolate(
    time: Res<Time<Real>>,
    mut remotes: Query<(&mut RemotePlayer, &mut Transform, &Children)>,
    mut pivots: Query<(&mut Transform, &Children), (Part<HeadPivot>, Without<Body>, Without<Gun>)>,
    mut bodies: Query<&mut Transform, (Part<Body>, Without<HeadPivot>, Without<Gun>)>,
    mut guns: Query<(&mut Transform, &mut GunHolder), (Part<Gun>, Without<HeadPivot>, Without<Body>)>,
) {
    let render_time = time.elapsed_secs_f64() - INTERP_DELAY;
    let ease = (time.delta_secs() * 12.0).min(1.0);
    let crouch_drop = PLAYER_HALF.y - CROUCH_HALF.y;
    // (body height, head height) when fully crouched: both shrink by twice the
    // drop of the box centre, because the feet stay where they are.
    let crouched_body = (BODY_HEIGHT - 2.0 * crouch_drop) / BODY_HEIGHT;
    for (mut remote, mut transform, children) in &mut remotes {
        let samples = &remote.samples;
        let Some(newest) = samples.back() else {
            continue;
        };
        // Find the pair of samples around the render time.
        let after = samples.iter().position(|s| s.time >= render_time);
        let (pos, yaw, pitch) = match after {
            Some(0) => (samples[0].pos, samples[0].yaw, samples[0].pitch),
            Some(i) => {
                let (a, b) = (&samples[i - 1], &samples[i]);
                let t = ((render_time - a.time) / (b.time - a.time).max(1e-6)) as f32;
                (
                    a.pos.lerp(b.pos, t),
                    lerp_angle(a.yaw, b.yaw, t),
                    a.pitch + (b.pitch - a.pitch) * t,
                )
            }
            // Everything is older than the render time: hold the newest sample.
            None => (newest.pos, newest.yaw, newest.pitch),
        };
        transform.translation = pos;
        transform.rotation = Quat::from_rotation_y(yaw);
        remote.pitch = pitch;
        let (crouch, reload) = (remote.crouching as u8 as f32, remote.reloading as u8 as f32);
        remote.crouch_pose += (crouch - remote.crouch_pose) * ease;
        remote.reload_pose += (reload - remote.reload_pose) * ease;

        // The reported position is the centre of their collision box, which is
        // the short one while they crouch. Pose the model around it.
        let pose = remote.crouch_pose;
        // How far the model's feet sit below the reported centre.
        let feet = -(PLAYER_HALF.y - crouch_drop * remote.crouching as u8 as f32);
        let body_scale = 1.0 + (crouched_body - 1.0) * pose;
        let body_height = BODY_HEIGHT * body_scale;
        for child in children.iter() {
            if let Ok(mut body) = bodies.get_mut(child) {
                body.translation.y = feet + body_height / 2.0;
                body.scale.y = body_scale;
            } else if let Ok((mut pivot, parts)) = pivots.get_mut(child) {
                pivot.translation.y = feet + body_height + NECK;
                pivot.rotation = Quat::from_rotation_x(remote.pitch);
                for part in parts.iter() {
                    if let Ok((mut gun, mut holder)) = guns.get_mut(part) {
                        holder.weapon = remote.weapon;
                        gun.translation = GUN_REST + Vec3::new(0.0, -0.12, 0.1) * remote.reload_pose;
                        gun.rotation = Quat::from_rotation_x(-0.9 * remote.reload_pose);
                    }
                }
            }
        }
    }
}

/// Weapons without a model are a single box the size of the gun.
fn size_gun_boxes(
    fitted_models: Res<FittedModels>,
    guns: Query<(&GunHolder, &Children), With<Gun>>,
    mut boxes: Query<(&mut Transform, &mut Visibility), With<GunBox>>,
) {
    for (holder, children) in &guns {
        let modelled = fitted_models.get(holder.weapon).is_some();
        for child in children.iter() {
            if let Ok((mut transform, mut visibility)) = boxes.get_mut(child) {
                transform.scale = Vec3::from(weapon(holder.weapon).size);
                visibility.set_if_neq(if modelled {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                });
            }
        }
    }
}

fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let diff = (b - a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    a + diff * t
}
