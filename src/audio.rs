//! Sound. There are no audio files: every effect is synthesised at startup
//! and handed to Bevy as an in-memory WAV. Other players' sounds are spatial,
//! so you can hear where shots and footsteps come from.

use std::{collections::HashMap, f32::consts::TAU};

use bevy::{
    audio::{SpatialScale, Volume},
    prelude::*,
};

use crate::{
    AppState,
    player::{LocalPlayer, PlayerSystems},
    protocol::PlayerId,
    weapons::{WeaponId, weapon},
    remote::{RemotePlayer, RemoteSystems},
};

const SAMPLE_RATE: u32 = 44_100;
/// Metres walked between two footstep sounds.
const STRIDE: f32 = 2.6;
/// Slower than this is treated as standing still.
const STEP_MIN_SPEED: f32 = 2.0;
/// Spatial sounds are at full volume inside this many metres, then fall off
/// with the square of the distance.
const GUNSHOT_FULL_VOLUME_RANGE: f32 = 10.0;
const EXPLOSION_FULL_VOLUME_RANGE: f32 = 16.0;
const FOOTSTEP_FULL_VOLUME_RANGE: f32 = 4.0;

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SoundCue>()
            .insert_resource(GlobalVolume::new(Volume::Linear(0.64)))
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    (local_footsteps, remote_footsteps)
                        .after(PlayerSystems::Simulate)
                        .after(RemoteSystems)
                        .run_if(in_state(AppState::Playing)),
                    play_cues,
                )
                    .chain(),
            );
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Sound {
    /// Each weapon's shot is the same recording, played deeper or higher.
    Gunshot(WeaponId),
    /// The tick of one of our bullets reaching somebody.
    Hit,
    /// The same, sharper and louder, for a headshot.
    Headshot,
    /// The chime of a kill.
    Kill,
    Footstep,
    /// Hitting the ground after a jump or fall.
    Land,
    /// The clack of a magazine going out or in.
    Reload,
    /// The swish of a grenade being thrown.
    Throw,
    /// A grenade knocking against something.
    Bounce,
    Explosion,
}

/// Ask for a sound to be played. `at: None` is our own sound (not spatial);
/// `Some(position)` is somebody else's, heard from that point in the world.
#[derive(Message)]
pub struct SoundCue {
    pub sound: Sound,
    pub at: Option<Vec3>,
}

#[derive(Resource)]
struct Sounds {
    gunshot: Handle<AudioSource>,
    footsteps: Vec<Handle<AudioSource>>,
    clack: Handle<AudioSource>,
    swish: Handle<AudioSource>,
    explosion: Handle<AudioSource>,
    tick: Handle<AudioSource>,
    chime: Handle<AudioSource>,
    /// State of the tiny random generator used to vary pitch.
    rng: u32,
}

fn setup(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
    let mut add = |samples: Vec<f32>| {
        sources.add(AudioSource {
            bytes: wav_bytes(&samples).into(),
        })
    };
    commands.insert_resource(Sounds {
        gunshot: add(synth_gunshot()),
        footsteps: (0..4).map(|variant| add(synth_footstep(variant))).collect(),
        clack: add(synth_clack()),
        swish: add(synth_swish()),
        explosion: add(synth_explosion()),
        tick: add(synth_tick()),
        chime: add(synth_chime()),
        rng: 0x2545_f491,
    });
}

fn play_cues(mut commands: Commands, mut cues: MessageReader<SoundCue>, mut sounds: ResMut<Sounds>) {
    for cue in cues.read() {
        let random = next_random(&mut sounds.rng);
        let own = cue.at.is_none();
        // (sound, volume, playback speed)
        let (source, volume, speed) = match cue.sound {
            Sound::Gunshot(weapon_id) => (
                sounds.gunshot.clone(),
                if own { 0.5 } else { 1.0 },
                (0.94 + random * 0.12) * weapon(weapon_id).pitch,
            ),
            Sound::Hit => (sounds.tick.clone(), 0.55, 0.96 + random * 0.08),
            Sound::Headshot => (sounds.tick.clone(), 0.85, 1.6),
            Sound::Kill => (sounds.chime.clone(), 0.6, 1.0),
            Sound::Footstep => (
                sounds.footsteps[(random * 97.0) as usize % sounds.footsteps.len()].clone(),
                if own { 0.3 } else { 1.0 },
                0.9 + random * 0.25,
            ),
            Sound::Land => (
                sounds.footsteps[0].clone(),
                if own { 0.55 } else { 1.0 },
                0.62 + random * 0.08,
            ),
            Sound::Reload => (sounds.clack.clone(), 0.5, 0.95 + random * 0.1),
            Sound::Throw => (sounds.swish.clone(), 0.5, 0.95 + random * 0.1),
            Sound::Bounce => (sounds.clack.clone(), 0.8, 1.5 + random * 0.3),
            Sound::Explosion => (sounds.explosion.clone(), 1.0, 0.92 + random * 0.16),
        };
        let settings = PlaybackSettings::DESPAWN
            .with_volume(Volume::Linear(volume))
            .with_speed(speed);
        match cue.at {
            None => {
                commands.spawn((AudioPlayer::new(source), settings));
            }
            Some(position) => {
                let range = match cue.sound {
                    Sound::Gunshot(_) => GUNSHOT_FULL_VOLUME_RANGE,
                    Sound::Explosion => EXPLOSION_FULL_VOLUME_RANGE,
                    _ => FOOTSTEP_FULL_VOLUME_RANGE,
                };
                commands.spawn((
                    AudioPlayer::new(source),
                    settings
                        .with_spatial(true)
                        .with_spatial_scale(SpatialScale::new(1.0 / range)),
                    Transform::from_translation(position),
                ));
            }
        }
    }
}

/// Our own steps: one per stride while running on the ground, plus a thump on landing.
fn local_footsteps(
    player: Res<LocalPlayer>,
    time: Res<Time>,
    mut cues: MessageWriter<SoundCue>,
    mut stride: Local<f32>,
    mut airborne_for: Local<f32>,
) {
    if !player.alive {
        *stride = 0.0;
        *airborne_for = 0.0;
        return;
    }
    let dt = time.delta_secs();
    if !player.grounded {
        *airborne_for += dt;
        // Start the next stride half way, so the first step after landing comes quickly.
        *stride = STRIDE * 0.5;
        return;
    }
    if *airborne_for > 0.25 {
        cues.write(SoundCue {
            sound: Sound::Land,
            at: None,
        });
    }
    *airborne_for = 0.0;

    let speed = player.horizontal_speed();
    // Crouch-walking and sliding are silent.
    if speed < STEP_MIN_SPEED || player.crouching {
        *stride = STRIDE * 0.5;
        return;
    }
    *stride += speed * dt;
    if *stride >= STRIDE {
        *stride -= STRIDE;
        cues.write(SoundCue {
            sound: Sound::Footstep,
            at: None,
        });
    }
}

#[derive(Default)]
struct RemoteStride {
    last_pos: Vec3,
    stride: f32,
    airborne_for: f32,
}

/// Other players' steps. The server does not tell us whether they are on the
/// ground, so that is inferred from how they move: running without rising or falling.
fn remote_footsteps(
    time: Res<Time>,
    remotes: Query<(&RemotePlayer, &Transform)>,
    mut cues: MessageWriter<SoundCue>,
    mut strides: Local<HashMap<PlayerId, RemoteStride>>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    strides.retain(|id, _| remotes.iter().any(|(remote, _)| remote.id == *id));

    for (remote, transform) in &remotes {
        let pos = transform.translation;
        let state = strides.entry(remote.id).or_insert_with(|| RemoteStride {
            last_pos: pos,
            ..default()
        });
        let moved = pos - state.last_pos;
        state.last_pos = pos;
        // Dead, or teleported by a respawn.
        if !remote.alive || moved.length() > 3.0 {
            state.stride = 0.0;
            state.airborne_for = 0.0;
            continue;
        }
        let feet = pos - Vec3::Y * crate::map::PLAYER_HALF.y;
        let vertical_speed = moved.y.abs() / dt;
        if vertical_speed > 1.5 {
            state.airborne_for += dt;
            state.stride = STRIDE * 0.5;
            continue;
        }
        if state.airborne_for > 0.25 {
            cues.write(SoundCue {
                sound: Sound::Land,
                at: Some(feet),
            });
        }
        state.airborne_for = 0.0;

        let travelled = moved.with_y(0.0).length();
        if travelled / dt < STEP_MIN_SPEED || remote.crouching {
            state.stride = STRIDE * 0.5;
            continue;
        }
        state.stride += travelled;
        if state.stride >= STRIDE {
            state.stride -= STRIDE;
            cues.write(SoundCue {
                sound: Sound::Footstep,
                at: Some(feet),
            });
        }
    }
}

// ---------------------------------------------------------------- synthesis

/// A short, bright blip: easy to hear through gunfire.
fn synth_tick() -> Vec<f32> {
    samples(0.05, |t| {
        let tone = (TAU * 2400.0 * t).sin() + 0.5 * (TAU * 3600.0 * t).sin();
        tone * (-t * 90.0).exp() * (t * 2000.0).min(1.0) * 0.6
    })
}

/// Two rising notes.
fn synth_chime() -> Vec<f32> {
    samples(0.4, |t| {
        let note = |start: f32, frequency: f32| {
            if t < start {
                return 0.0;
            }
            let since = t - start;
            let tone = (TAU * frequency * since).sin() + 0.3 * (TAU * frequency * 2.0 * since).sin();
            tone * (-since * 11.0).exp() * (since * 800.0).min(1.0)
        };
        (note(0.0, 880.0) + note(0.09, 1318.5)) * 0.5
    })
}

/// Two hard little knocks: a magazine latch, or a grenade hitting concrete.
fn synth_clack() -> Vec<f32> {
    let mut rng = 0x0bad_cafe;
    let mut low_passed = 0.0;
    samples(0.09, |t| {
        let noise = next_random(&mut rng) * 2.0 - 1.0;
        low_passed += (noise - low_passed) * 0.45;
        // The second knock follows 35 ms after the first.
        let knock = |at: f32| if t >= at { (-(t - at) * 160.0).exp() } else { 0.0 };
        let envelope = knock(0.0) + knock(0.035) * 0.7;
        let ring = (TAU * 1900.0 * t).sin() * 0.35;
        ((low_passed * 1.6 + ring) * envelope).tanh() * 0.8
    })
}

/// A breath of noise that rises and falls.
fn synth_swish() -> Vec<f32> {
    let mut rng = 0x5eed_1234;
    let mut low_passed = 0.0;
    samples(0.22, |t| {
        let noise = next_random(&mut rng) * 2.0 - 1.0;
        let progress = t / 0.22;
        // The filter opens up as the arm speeds up.
        low_passed += (noise - low_passed) * (0.05 + 0.3 * progress);
        let envelope = (progress * std::f32::consts::PI).sin().powi(2);
        (low_passed * envelope * 2.4).tanh() * 0.7
    })
}

/// A hard crack, then a long rumble that gets duller as it dies away.
fn synth_explosion() -> Vec<f32> {
    let mut rng = 0x00c0_ffee;
    let mut low_passed = 0.0;
    let mut rumble = 0.0;
    let mut phase = 0.0_f32;
    samples(1.3, |t| {
        let noise = next_random(&mut rng) * 2.0 - 1.0;
        let brightness = 0.02 + 0.7 * (-t * 18.0).exp();
        low_passed += (noise - low_passed) * brightness;
        let crack = low_passed * (-t * 9.0).exp() * 2.2;

        rumble += (noise - rumble) * 0.012;
        let tail = rumble * (-t * 3.2).exp() * 9.0;

        let frequency = 38.0 + 70.0 * (-t * 14.0).exp();
        phase += TAU * frequency / SAMPLE_RATE as f32;
        let thump = phase.sin() * (-t * 5.5).exp() * 1.3;

        (crack + tail + thump).tanh() * 0.95
    })
}

/// A short crack of noise over a falling bass thump.
fn synth_gunshot() -> Vec<f32> {
    let mut rng = 0x9e37_79b9;
    let mut low_passed = 0.0;
    let mut phase = 0.0_f32;
    samples(0.26, |t| {
        let noise = next_random(&mut rng) * 2.0 - 1.0;
        // The noise starts bright and gets duller as it rings out.
        let brightness = 0.08 + 0.8 * (-t * 45.0).exp();
        low_passed += (noise - low_passed) * brightness;
        let crack = low_passed * (-t * 24.0).exp();

        let frequency = 55.0 + 150.0 * (-t * 38.0).exp();
        phase += TAU * frequency / SAMPLE_RATE as f32;
        let thump = phase.sin() * (-t * 17.0).exp();

        // Soft clipping gives it some bite.
        ((crack * 1.5 + thump * 0.9) * 1.6).tanh() * 0.85
    })
}

/// A dull, short thud. Each variant is filtered and pitched a little differently.
fn synth_footstep(variant: u32) -> Vec<f32> {
    let mut rng = 0x1234_5678 + variant * 7919;
    let mut low_passed = 0.0;
    let softness = 0.07 + 0.025 * variant as f32;
    let tone = 75.0 + 12.0 * variant as f32;
    samples(0.12, |t| {
        let noise = next_random(&mut rng) * 2.0 - 1.0;
        low_passed += (noise - low_passed) * softness;
        let scuff = low_passed * (-t * 38.0).exp() * 2.2;
        let thud = (TAU * tone * t).sin() * (-t * 50.0).exp() * 0.7;
        // A few milliseconds of fade-in avoids a click.
        (scuff + thud).tanh() * (t * 400.0).min(1.0) * 0.8
    })
}

fn samples(seconds: f32, mut wave: impl FnMut(f32) -> f32) -> Vec<f32> {
    let count = (seconds * SAMPLE_RATE as f32) as usize;
    (0..count)
        .map(|i| {
            // Fade the tail to exactly zero.
            let fade_out = ((count - 1 - i) as f32 / 400.0).min(1.0);
            wave(i as f32 / SAMPLE_RATE as f32) * fade_out
        })
        .collect()
}

/// Wraps mono samples in a 16-bit PCM WAV file.
fn wav_bytes(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes()); // format chunk size
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // bytes per second
    bytes.extend_from_slice(&2u16.to_le_bytes()); // bytes per frame
    bytes.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// xorshift32, returned as a float in 0..1.
fn next_random(state: &mut u32) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state >> 8) as f32 / (1u32 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sounds_are_audible_and_do_not_clip() {
        let mut all = vec![
            synth_gunshot(),
            synth_clack(),
            synth_swish(),
            synth_explosion(),
            synth_tick(),
            synth_chime(),
        ];
        all.extend((0..4).map(synth_footstep));
        for sound in all {
            let peak = sound.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()));
            assert!(peak > 0.3 && peak <= 1.0, "peak {peak}");
            assert!(sound.iter().all(|s| s.is_finite()));
            assert_eq!(*sound.last().unwrap(), 0.0);
            assert_eq!(wav_bytes(&sound).len(), 44 + sound.len() * 2);
        }
    }
}
