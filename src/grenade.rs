//! Grenades as the client sees them. The server owns the real ones (and the
//! damage); it tells us when one is thrown and when it goes off. In between we
//! run the same bounce simulation, so the grenade on screen is where the
//! server's is.

use bevy::{light::NotShadowCaster, prelude::*};

use crate::{
    AppState,
    audio::{Sound, SoundCue},
    map::GRENADE_HALF,
    net::NetMsg,
    protocol::*,
    world::{GameMap, SharedAssets},
};

/// Seconds the fireball lasts.
const BLAST_TIME: f32 = 0.45;
/// Widest the fireball gets, in metres across.
const BLAST_SIZE: f32 = 5.5;
const BLAST_LIGHT: f32 = 6_000_000.0;

pub struct GrenadePlugin;

impl Plugin for GrenadePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (handle_net, simulate, update_blasts)
                .chain()
                .run_if(in_state(AppState::Playing)),
        );
    }
}

#[derive(Component)]
struct Grenade {
    id: u32,
    pos: Vec3,
    /// Where it was one simulation step ago, to draw it smoothly in between.
    previous: Vec3,
    vel: Vec3,
    /// Time not yet consumed by whole simulation steps.
    clock: f32,
    age: f32,
}

#[derive(Component)]
struct Blast {
    age: f32,
}

fn handle_net(
    mut commands: Commands,
    mut messages: MessageReader<NetMsg>,
    assets: Res<SharedAssets>,
    grenades: Query<(Entity, &Grenade)>,
    mut sounds: MessageWriter<SoundCue>,
) {
    for NetMsg(msg) in messages.read() {
        match msg {
            ServerMsg::Grenade { id, pos, vel } => {
                let pos = Vec3::from(*pos);
                commands.spawn((
                    Grenade {
                        id: *id,
                        pos,
                        previous: pos,
                        vel: Vec3::from(*vel),
                        clock: 0.0,
                        age: 0.0,
                    },
                    Mesh3d(assets.sphere.clone()),
                    MeshMaterial3d(assets.grenade.clone()),
                    Transform::from_translation(pos).with_scale(Vec3::splat(GRENADE_HALF * 2.0)),
                    DespawnOnExit(AppState::Playing),
                ));
            }
            ServerMsg::Explosion { id, pos } => {
                for (entity, grenade) in &grenades {
                    if grenade.id == *id {
                        commands.entity(entity).despawn();
                    }
                }
                let pos = Vec3::from(*pos);
                sounds.write(SoundCue {
                    sound: Sound::Explosion,
                    at: Some(pos),
                });
                commands.spawn((
                    Blast { age: 0.0 },
                    Mesh3d(assets.sphere.clone()),
                    MeshMaterial3d(assets.explosion.clone()),
                    Transform::from_translation(pos).with_scale(Vec3::splat(0.3)),
                    NotShadowCaster,
                    DespawnOnExit(AppState::Playing),
                ));
                // The flash. Not a child of the fireball, whose scale would shrink it.
                commands.spawn((
                    Blast { age: 0.0 },
                    PointLight {
                        color: Color::srgb(1.0, 0.6, 0.25),
                        intensity: BLAST_LIGHT,
                        range: 40.0,
                        ..default()
                    },
                    Transform::from_translation(pos + Vec3::Y * 0.8),
                    DespawnOnExit(AppState::Playing),
                ));
            }
            _ => {}
        }
    }
}

fn simulate(
    mut commands: Commands,
    time: Res<Time>,
    map: Res<GameMap>,
    mut grenades: Query<(Entity, &mut Grenade, &mut Transform)>,
    mut sounds: MessageWriter<SoundCue>,
) {
    for (entity, mut grenade, mut transform) in &mut grenades {
        let grenade = &mut *grenade;
        grenade.clock += time.delta_secs().min(0.25);
        while grenade.clock >= GRENADE_STEP {
            grenade.clock -= GRENADE_STEP;
            grenade.age += GRENADE_STEP;
            grenade.previous = grenade.pos;
            if map.0.step_grenade(&mut grenade.pos, &mut grenade.vel, GRENADE_STEP) {
                sounds.write(SoundCue {
                    sound: Sound::Bounce,
                    at: Some(grenade.pos),
                });
            }
        }
        transform.translation = grenade.previous.lerp(grenade.pos, grenade.clock / GRENADE_STEP);
        // The server's explosion normally removes it. This only tidies up one
        // whose explosion message never came.
        if grenade.age > GRENADE_FUSE + 2.0 {
            commands.entity(entity).despawn();
        }
    }
}

/// The fireball swells fast and then shrinks away; the flash fades with it.
fn update_blasts(
    mut commands: Commands,
    time: Res<Time>,
    mut blasts: Query<(Entity, &mut Blast, &mut Transform, Option<&mut PointLight>)>,
) {
    for (entity, mut blast, mut transform, light) in &mut blasts {
        blast.age += time.delta_secs();
        let t = blast.age / BLAST_TIME;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        match light {
            Some(mut light) => light.intensity = BLAST_LIGHT * (1.0 - t) * (1.0 - t),
            None => {
                let swell = (t / 0.3).min(1.0);
                let size = if t < 0.3 {
                    1.0 - (1.0 - swell).powi(3)
                } else {
                    1.0 - ((t - 0.3) / 0.7).powi(2)
                };
                transform.scale = Vec3::splat((BLAST_SIZE * size).max(0.01));
            }
        }
    }
}
