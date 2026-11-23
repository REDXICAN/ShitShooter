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
    /// The clack of a weapon being taken out.
    Reload,
    /// A magazine released and sliding out.
    MagOut,
    /// A magazine pushed in until it latches.
    MagIn,
    /// A bolt or slide snapping forward at the end of a reload.
    Bolt,
    /// A shell pushed into a shotgun's tube.
    ShellIn,
    /// A shotgun's pump racked back and forth.
    Pump,
    /// The click of a grenade's pin being pulled.
    Pin,
    /// The swish of a grenade being thrown.
    Throw,
    /// A grenade knocking against something.
    Bounce,
    Explosion,
    /// A flashbang's sharp bang.
    Flashbang,
    /// A smoke grenade bursting open with a hiss.
    SmokePop,
    /// The ringing in our ears after being flashed.
    Ringing,
    /// We took somebody's last armor.
    ArmorBreak,
    /// Somebody took our last armor.
    ArmorBroken,
    /// Armor picked up: ours, or heard from where somebody else took it.
    ArmorPickup,
    /// One second of the countdown before a match.
    Countdown,
    /// The countdown is over: go.
    Go,
    /// We or a teammate pinged something.
    Ping,
    /// A soft click: the gun in hand is fully choked.
    Choke,
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
    shatter: Handle<AudioSource>,
    charge: Handle<AudioSource>,
    hiss: Handle<AudioSource>,
    ring: Handle<AudioSource>,
    beep: Handle<AudioSource>,
    blip: Handle<AudioSource>,
    click: Handle<AudioSource>,
    mag_out: Handle<AudioSource>,
    mag_in: Handle<AudioSource>,
    bolt: Handle<AudioSource>,
    shell_in: Handle<AudioSource>,
    pump: Handle<AudioSource>,
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
        shatter: add(synth_shatter()),
        charge: add(synth_charge()),
        hiss: add(synth_hiss()),
        ring: add(synth_ring()),
        beep: add(synth_beep()),
        blip: add(synth_blip()),
        click: add(synth_click()),
        mag_out: add(synth_mag_out()),
        mag_in: add(synth_mag_in()),
        bolt: add(synth_bolt()),
        shell_in: add(synth_shell_in()),
        pump: add(synth_pump()),
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
            Sound::Pin => (sounds.clack.clone(), 0.35, 2.4 + random * 0.2),
            Sound::Throw => (sounds.swish.clone(), 0.5, 0.95 + random * 0.1),
            Sound::Bounce => (sounds.clack.clone(), 0.8, 1.5 + random * 0.3),
            Sound::Explosion => (sounds.explosion.clone(), 1.0, 0.92 + random * 0.16),
            // Sped up, the explosion turns into a short, hard crack.
            Sound::Flashbang => (sounds.explosion.clone(), 1.0, 1.9 + random * 0.1),
            Sound::SmokePop => (sounds.hiss.clone(), 1.0, 0.95 + random * 0.1),
            Sound::Ringing => (sounds.ring.clone(), 0.35, 1.0),
            Sound::ArmorBreak => (sounds.shatter.clone(), 0.75, 1.2 + random * 0.06),
            // Played low and slow it sounds heavier: something of ours gave way.
            Sound::ArmorBroken => (sounds.shatter.clone(), 1.0, 0.68),
            Sound::ArmorPickup => (sounds.charge.clone(), if own { 0.6 } else { 1.0 }, 1.0),
            Sound::Countdown => (sounds.beep.clone(), 0.45, 1.0),
            // An octave up, and louder.
            Sound::Go => (sounds.beep.clone(), 0.7, 2.0),
            Sound::Ping => (sounds.blip.clone(), 0.5, 1.0),
            Sound::Choke => (sounds.click.clone(), 0.3, 1.0),
            Sound::MagOut => (sounds.mag_out.clone(), 0.55, 0.95 + random * 0.1),
            Sound::MagIn => (sounds.mag_in.clone(), 0.6, 0.95 + random * 0.1),
            Sound::Bolt => (sounds.bolt.clone(), 0.6, 0.96 + random * 0.08),
            Sound::ShellIn => (sounds.shell_in.clone(), 0.5, 0.92 + random * 0.16),
            Sound::Pump => (sounds.pump.clone(), 0.75, 0.97 + random * 0.06),
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
                    Sound::Explosion | Sound::Flashbang => EXPLOSION_FULL_VOLUME_RANGE,
                    Sound::SmokePop => GUNSHOT_FULL_VOLUME_RANGE,
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

/// A metal knock: a decaying burst of filtered noise with a ring at
/// `frequency`, starting `at` seconds in. The building block of the reload
/// sounds.
fn knock(t: f32, at: f32, decay: f32, frequency: f32, noise: f32) -> f32 {
    if t < at {
        return 0.0;
    }
    let since = t - at;
    let envelope = (-since * decay).exp() * (since * 4000.0).min(1.0);
    (noise * 1.3 + (TAU * frequency * since).sin() * 0.6) * envelope
}

/// A magazine released: the catch clicks, then it scrapes out of the well.
fn synth_mag_out() -> Vec<f32> {
    let mut rng = 0x3a6_0001;
    let mut low_passed = 0.0;
    samples(0.2, |t| {
        let noise = next_random(&mut rng) * 2.0 - 1.0;
        low_passed += (noise - low_passed) * 0.25;
        let scrape = low_passed * (t * 40.0).min(1.0) * (-(t - 0.02).max(0.0) * 18.0).exp() * 0.8;
        ((knock(t, 0.0, 220.0, 2300.0, noise) + scrape) * 1.4).tanh() * 0.8
    })
}

/// A magazine pushed home: a dull thud, then the catch latching.
fn synth_mag_in() -> Vec<f32> {
    let mut rng = 0x3a6_0002;
    let mut low_passed = 0.0;
    samples(0.16, |t| {
        let noise = next_random(&mut rng) * 2.0 - 1.0;
        low_passed += (noise - low_passed) * 0.18;
        let thud = knock(t, 0.0, 70.0, 180.0, low_passed * 2.0);
        let latch = knock(t, 0.055, 260.0, 2600.0, noise);
        ((thud + latch * 0.8) * 1.5).tanh() * 0.85
    })
}

/// A bolt or slide let go: a sharp metal slam with a bright ring.
fn synth_bolt() -> Vec<f32> {
    let mut rng = 0x3a6_0003;
    samples(0.18, |t| {
        let noise = next_random(&mut rng) * 2.0 - 1.0;
        let slam = knock(t, 0.0, 120.0, 1700.0, noise) + knock(t, 0.012, 60.0, 3100.0, 0.0) * 0.5;
        (slam * 1.6).tanh() * 0.85
    })
}

/// A shell thumbed into the tube: a short, soft knock.
fn synth_shell_in() -> Vec<f32> {
    let mut rng = 0x3a6_0004;
    let mut low_passed = 0.0;
    samples(0.1, |t| {
        let noise = next_random(&mut rng) * 2.0 - 1.0;
        low_passed += (noise - low_passed) * 0.3;
        let push = knock(t, 0.0, 140.0, 900.0, low_passed * 1.5) + knock(t, 0.03, 200.0, 1500.0, noise) * 0.6;
        (push * 1.5).tanh() * 0.75
    })
}

/// A pump racked: back with a scrape and a clack, then forward with a heavier one.
fn synth_pump() -> Vec<f32> {
    let mut rng = 0x3a6_0005;
    let mut low_passed = 0.0;
    samples(0.32, |t| {
        let noise = next_random(&mut rng) * 2.0 - 1.0;
        low_passed += (noise - low_passed) * 0.2;
        let slide = |from: f32, to: f32| if (from..to).contains(&t) { low_passed * 0.5 } else { 0.0 };
        let back = knock(t, 0.09, 110.0, 1300.0, noise) + slide(0.0, 0.09);
        let forward = knock(t, 0.22, 90.0, 900.0, noise) + slide(0.14, 0.22);
        ((back + forward * 1.2) * 1.5).tanh() * 0.9
    })
}

/// A soft, dry click, like a dial turning one notch.
fn synth_click() -> Vec<f32> {
    let mut rng = 0x00c1_1c4e;
    let mut low_passed = 0.0;
    samples(0.045, |t| {
        let noise = next_random(&mut rng) * 2.0 - 1.0;
        low_passed += (noise - low_passed) * 0.3;
        let knock = (low_passed * 1.4 + (TAU * 1350.0 * t).sin() * 0.5) * (-t * 170.0).exp();
        (knock * 1.8).tanh() * (t * 4000.0).min(1.0) * 0.7
    })
}

/// A short, plain electronic beep.
fn synth_beep() -> Vec<f32> {
    samples(0.18, |t| {
        let tone = (TAU * 740.0 * t).sin() + 0.25 * (TAU * 1480.0 * t).sin();
        tone * (t * 600.0).min(1.0) * ((0.18 - t) * 40.0).min(1.0) * 0.45
    })
}

/// Two quick high notes, falling: "over here".
fn synth_blip() -> Vec<f32> {
    samples(0.22, |t| {
        let note = |start: f32, frequency: f32| {
            if t < start {
                return 0.0;
            }
            let since = t - start;
            (TAU * frequency * since).sin() * (-since * 30.0).exp() * (since * 1500.0).min(1.0)
        };
        (note(0.0, 1975.5) + note(0.06, 1318.5)) * 0.45
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

/// Armor shattering: a sharp crack of bright noise, glassy shards ringing
/// out one after another, and a dull thump as the plate gives way.
fn synth_shatter() -> Vec<f32> {
    let mut rng = 0x51ab_0001;
    let mut low_passed = 0.0;
    // (frequency, start, decay): inharmonic, so it rings like a shard, not a note.
    const SHARDS: [(f32, f32, f32); 5] = [
        (2350.0, 0.0, 22.0),
        (3170.0, 0.012, 26.0),
        (1720.0, 0.03, 18.0),
        (4410.0, 0.045, 30.0),
        (2890.0, 0.07, 24.0),
    ];
    samples(0.45, |t| {
        let noise = next_random(&mut rng) * 2.0 - 1.0;
        low_passed += (noise - low_passed) * 0.5;
        // What the low pass took out: only the hiss.
        let crack = (noise - low_passed) * (-t * 28.0).exp() * 1.6;
        let ring: f32 = SHARDS
            .iter()
            .filter(|(_, start, _)| t >= *start)
            .map(|(frequency, start, decay)| {
                let since = t - start;
                (TAU * frequency * since).sin() * (-since * decay).exp() * (since * 3000.0).min(1.0)
            })
            .sum();
        let thump = (TAU * 140.0 * t).sin() * (-t * 25.0).exp();
        (crack + ring * 0.4 + thump * 0.7).tanh() * (t * 2000.0).min(1.0) * 0.85
    })
}

/// A rising, shimmering sweep: armor charging up.
fn synth_charge() -> Vec<f32> {
    const LENGTH: f32 = 0.32;
    let mut phase = 0.0_f32;
    samples(LENGTH, |t| {
        let progress = t / LENGTH;
        let frequency = 520.0 + 900.0 * progress;
        phase += TAU * frequency / SAMPLE_RATE as f32;
        let tone = phase.sin() + 0.4 * (phase * 1.5).sin() + 0.25 * (phase * 2.0).sin();
        let shimmer = 0.75 + 0.25 * (TAU * 28.0 * t).sin();
        let envelope = (t * 200.0).min(1.0) * (1.0 - progress).powf(1.5);
        (tone * shimmer * envelope * 0.9).tanh() * 0.8
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

/// A pop, then a long hiss of gas that slowly dies down.
fn synth_hiss() -> Vec<f32> {
    let mut rng = 0x0dd5_e0a1;
    let mut low_passed = 0.0;
    let mut band = 0.0;
    samples(1.6, |t| {
        let noise = next_random(&mut rng) * 2.0 - 1.0;
        low_passed += (noise - low_passed) * 0.35;
        band += (low_passed - band) * 0.08;
        let pop = band * (-t * 30.0).exp() * 6.0;
        let hiss = (noise - low_passed) * (t * 30.0).min(1.0) * (-t * 1.6).exp() * 1.4;
        (pop + hiss).tanh() * 0.8
    })
}

/// A high, thin whine that fades slowly: ears ringing.
fn synth_ring() -> Vec<f32> {
    samples(3.5, |t| {
        let wobble = 1.0 + 0.002 * (TAU * 5.0 * t).sin();
        let tone = (TAU * 3150.0 * wobble * t).sin() + 0.2 * (TAU * 4720.0 * t).sin();
        tone * (t * 6.0).min(1.0) * (-t * 0.9).exp() * 0.75
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
            synth_shatter(),
            synth_charge(),
            synth_hiss(),
            synth_ring(),
            synth_click(),
            synth_mag_out(),
            synth_mag_in(),
            synth_bolt(),
            synth_shell_in(),
            synth_pump(),
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
