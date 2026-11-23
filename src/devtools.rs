//! Developer helpers: F12 screenshots, and a scripted self-test (`--selftest <dir>`)
//! that plays a short match against bots by itself and saves screenshots.

use std::{path::PathBuf, time::SystemTime};

use bevy::{
    prelude::*,
    ui_widgets::{SetSliderValue, Slider, SliderValueChange},
    render::view::screenshot::{Screenshot, save_to_disk},
};

use crate::{
    AppState, LaunchArgs,
    net::{Connection, MenuStatus, Session},
    paint::{MyPaint, PlayerModel},
    paint_editor::PaintEditor,
    player::{Controls, LocalPlayer, PlayerSystems},
    protocol::ClientMsg,
    remote::RemotePlayer,
    settings::SettingsMenu,
};

pub struct DevToolsPlugin;

impl Plugin for DevToolsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, screenshot_key);

        let selftest = app.world().resource::<LaunchArgs>().selftest.clone();
        if let Some(dir) = selftest {
            let dir = PathBuf::from(dir);
            let _ = std::fs::create_dir_all(&dir);
            app.insert_resource(SelfTest {
                dir,
                clock: 0.0,
                top_speed: 0.0,
                step: 0,
            })
            // Sounds are still created and decoded, just not heard.
            .insert_resource(GlobalVolume::new(bevy::audio::Volume::SILENT))
            .init_resource::<CueCount>()
            .add_systems(Update, count_cues)
            .add_systems(OnEnter(AppState::Playing), restart_clock)
            .add_systems(OnEnter(AppState::Lobby), restart_clock)
            .add_systems(
                Update,
                (
                    selftest_menu.run_if(in_state(AppState::Menu)),
                    selftest_lobby.run_if(in_state(AppState::Lobby)),
                    selftest_play
                        .before(PlayerSystems::Simulate)
                        .run_if(in_state(AppState::Playing)),
                ),
            );
        }
    }
}

fn screenshot_key(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>) {
    if keys.just_pressed(KeyCode::F12) {
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let _ = std::fs::create_dir_all("screenshots");
        let path = format!("screenshots/shot-{stamp}.png");
        info!("saving {path}");
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
}

#[derive(Resource)]
struct SelfTest {
    dir: PathBuf,
    /// Seconds spent in the current state.
    clock: f32,
    top_speed: f32,
    /// How far through the current state's script we are.
    step: u32,
}

impl SelfTest {
    /// True once, the first time the clock passes `at` for script step `step`.
    fn reached(&mut self, step: u32, at: f32) -> bool {
        if self.step == step && self.clock >= at {
            self.step += 1;
            true
        } else {
            false
        }
    }

    fn capture(&self, commands: &mut Commands, name: &str) {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(self.dir.join(name)));
    }
}

fn restart_clock(mut test: ResMut<SelfTest>) {
    test.clock = 0.0;
    test.step = 0;
}

fn selftest_menu(
    mut commands: Commands,
    time: Res<Time>,
    status: Res<MenuStatus>,
    mut test: ResMut<SelfTest>,
    mut exit: MessageWriter<AppExit>,
) {
    test.clock += time.delta_secs();
    if test.reached(0, 1.5) {
        println!("[selftest] menu status: '{}'", status.0);
        test.capture(&mut commands, "menu.png");
    }
    if test.reached(1, 2.5) {
        exit.write(AppExit::Success);
    }
}

fn selftest_lobby(
    mut commands: Commands,
    time: Res<Time>,
    session: Res<Session>,
    mut test: ResMut<SelfTest>,
    mut connection: ResMut<Connection>,
    mut exit: MessageWriter<AppExit>,
    mut paint: ResMut<MyPaint>,
    mut editor: ResMut<PaintEditor>,
    model: Res<PlayerModel>,
    sliders: Query<Entity, With<Slider>>,
) {
    test.clock += time.delta_secs();
    // The second visit to the lobby is after the match was ended: we are done.
    if session.last_result.is_some() {
        if test.reached(0, 1.0) {
            test.capture(&mut commands, "lobby_after.png");
        }
        if test.reached(1, 2.0) {
            println!("[selftest] finished: {:?}", session.last_result);
            exit.write(AppExit::Success);
        }
        return;
    }
    if test.reached(0, 0.5) {
        connection.send(&ClientMsg::AddBot);
        connection.send(&ClientMsg::AddBot);
        connection.send(&ClientMsg::AddBot);
        // Play on the second map, which also exercises switching maps.
        connection.send(&ClientMsg::NextMap);
    }
    if test.reached(1, 1.5) {
        test.capture(&mut commands, "lobby.png");
    }
    // Stripes on the paint job, and a look at them on the paint screen.
    if test.reached(2, 2.0) {
        let mut canvas = paint.canvas.clone();
        for (pixel, texel) in canvas.0.iter_mut().zip(&model.texels) {
            if let Some((point, _)) = texel
                && (point.y * 6.0).floor() as i32 % 2 == 0
            {
                *pixel = 1;
            }
        }
        paint.set(canvas);
        editor.open = true;
    }
    // The brush size slider (the only slider there is) moved to 12 cm.
    if test.reached(3, 2.5) {
        for entity in &sliders {
            commands.trigger(SetSliderValue {
                entity,
                change: SliderValueChange::Absolute(12.0),
            });
        }
    }
    if test.reached(4, 3.0) {
        test.capture(&mut commands, "paint.png");
    }
    if test.reached(5, 3.3) {
        editor.open = false;
    }
    if test.reached(6, 3.8) {
        println!("[selftest] roster: {:?}", session.roster.iter().map(|p| &p.name).collect::<Vec<_>>());
        connection.send(&ClientMsg::StartMatch);
    }
}

/// Runs around, aims at the nearest bot and holds the trigger, and tries
/// every other move once along the way.
fn selftest_play(
    mut commands: Commands,
    time: Res<Time>,
    session: Res<Session>,
    mut test: ResMut<SelfTest>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut player: ResMut<LocalPlayer>,
    mut controls: ResMut<Controls>,
    mut settings_menu: ResMut<SettingsMenu>,
    mut connection: ResMut<Connection>,
    remotes: Query<(&RemotePlayer, &Transform)>,
    cue_count: Res<CueCount>,
    mut last_target: Local<Option<(u64, Vec3)>>,
    mut captured: Local<Captured>,
) {
    // The script starts once the countdown is over.
    if session.frozen() {
        if session.countdown < 2.5 && !captured.countdown {
            captured.countdown = true;
            println!("[selftest] countdown {:.1}, alive {}", session.countdown, player.alive);
            test.capture(&mut commands, "countdown.png");
        }
        // In the team modes, a look at the nearest teammate: outlined, named.
        let eye = player.eye();
        let mate = remotes
            .iter()
            .filter(|(remote, _)| remote.alive && session.team_mode() && !session.enemies(session.my_id, remote.id))
            .map(|(_, transform)| transform.translation)
            .min_by(|a, b| a.distance(eye).total_cmp(&b.distance(eye)));
        if let Some(mate) = mate
            && session.countdown < 1.8
        {
            let to = mate - eye;
            player.yaw = (-to.x).atan2(-to.z);
            player.pitch = to.y.atan2(to.with_y(0.0).length());
            if session.countdown < 1.2 && !captured.ally {
                captured.ally = true;
                println!("[selftest] nearest teammate {:.1} m away", to.length());
                test.capture(&mut commands, "teammate.png");
            }
        }
        return;
    }
    if player.hit_marker > 0.0 && !captured.marker {
        captured.marker = true;
        test.capture(&mut commands, "hit.png");
    }
    if let Some(arc) = player.damage_arcs.first()
        && !captured.hurt
    {
        captured.hurt = true;
        println!(
            "[selftest] hurt from {:?}, {:.0} degrees off our aim",
            arc.from,
            player.bearing(arc.at).to_degrees()
        );
        test.capture(&mut commands, "hurt.png");
    }
    test.clock += time.delta_secs();
    let t = test.clock;
    test.top_speed = test.top_speed.max(player.horizontal_speed());

    keys.press(KeyCode::KeyW);
    keys.press(KeyCode::KeyD);
    // Run for a while, then bunny hop, so both footsteps and landings happen.
    if t > 3.5 {
        keys.press(KeyCode::Space);
    } else {
        keys.release(KeyCode::Space);
    }
    if (2.2..2.3).contains(&t) {
        keys.press(KeyCode::ShiftLeft);
    } else {
        keys.release(KeyCode::ShiftLeft);
    }
    // The trigger is held, except with the pistol, which wants a click per
    // shot, and around the clicks that cook and throw grenades.
    let pistol_click = (4.4..5.0).contains(&t) && (t * 4.0).fract() > 0.5;
    let grenade_click = [1.0..1.2, 1.45..1.5, 1.6..2.0, 2.05..2.1].iter().any(|window| window.contains(&t));
    if pistol_click || grenade_click {
        mouse.release(MouseButton::Left);
    } else {
        mouse.press(MouseButton::Left);
    }
    if (5.0..6.5).contains(&t) {
        keys.press(KeyCode::Tab);
    } else {
        keys.release(KeyCode::Tab);
    }
    let mut hold = |key: KeyCode, held: bool| {
        if held {
            keys.press(key);
        } else {
            keys.release(key);
        }
    };
    // A frag early on (taken out, a click to pull the pin, and lobbed with a
    // right-click), then the second one cooked and thrown with clicks, a
    // crouch that turns into a slide, a ping, and a reload by hand.
    hold(KeyCode::KeyG, (1.0..1.1).contains(&t) || (1.6..1.75).contains(&t));
    hold(KeyCode::KeyF, (3.0..3.05).contains(&t));
    hold(KeyCode::ControlLeft, (2.35..3.3).contains(&t));
    hold(KeyCode::KeyR, (6.6..6.7).contains(&t));
    // Every weapon of the default loadout in turn: pistol, knife, back to the rifle.
    hold(KeyCode::Digit2, (4.4..4.5).contains(&t));
    hold(KeyCode::Digit3, (5.0..5.1).contains(&t));
    hold(KeyCode::Digit1, (5.6..5.7).contains(&t));
    if (1.4..2.1).contains(&t) {
        mouse.press(MouseButton::Right);
    } else {
        mouse.release(MouseButton::Right);
    }
    if test.reached(0, 1.9) {
        println!("[selftest] aim {:.2} ammo {} grenades {:.2?}", player.aim, player.guns[0].ammo, player.grenades);
        test.capture(&mut commands, "aim.png");
    }
    if test.reached(1, 2.7) {
        println!(
            "[selftest] crouching {} sliding {} speed {:.1} eye height {:.2}",
            player.crouching,
            player.sliding,
            player.horizontal_speed(),
            player.eye().y - player.feet().y
        );
        test.capture(&mut commands, "slide.png");
    }

    let eye = player.eye();
    let nearest = remotes
        .iter()
        .filter(|(remote, _)| remote.alive && session.enemies(session.my_id, remote.id))
        .map(|(remote, transform)| (remote.id, transform.translation))
        .min_by(|a, b| a.1.distance(eye).total_cmp(&b.1.distance(eye)));
    if let Some((id, target)) = nearest {
        // Bullets take time to arrive: aim where the target will be by then.
        let velocity = match *last_target {
            Some((last_id, last_pos)) if last_id == id && time.delta_secs() > 0.0 => {
                ((target - last_pos) / time.delta_secs()).clamp_length_max(12.0)
            }
            _ => Vec3::ZERO,
        };
        *last_target = Some((id, target));
        let flight = target.distance(eye) / player.weapon().speed.max(1.0);
        let to = target + velocity * flight - eye;
        player.yaw = (-to.x).atan2(-to.z);
        player.pitch = to.y.atan2(to.with_y(0.0).length());
    }

    if test.reached(2, 3.3) {
        test.capture(&mut commands, "ping.png");
    }
    if test.reached(3, 3.6) {
        test.capture(&mut commands, "play.png");
    }
    if test.reached(4, 6.0) {
        test.capture(&mut commands, "scores.png");
    }
    if test.reached(5, 7.5) {
        controls.captured = false;
    }
    if test.reached(6, 8.0) {
        test.capture(&mut commands, "pause.png");
    }
    // A screenshot is taken a frame or two after it is asked for, so the
    // settings open a moment later, not at once.
    if test.reached(7, 8.3) {
        settings_menu.open = true;
    }
    if test.reached(8, 8.8) {
        test.capture(&mut commands, "settings.png");
    }
    if test.reached(9, 9.2) {
        settings_menu.open = false;
    }
    if test.reached(10, 9.6) {
        let me = session.roster.iter().find(|p| p.id == session.my_id);
        println!(
            "[selftest] pos {:.1?} top speed {:.1} health {} armor {} me {:?} ping {:.1}ms",
            player.pos,
            test.top_speed,
            player.health,
            player.armor,
            me.map(|p| (p.kills, p.deaths)),
            connection.ping_ms()
        );
        println!(
            "[selftest] ammo {} reloading {} grenades {:.2?}",
            player.guns[0].ammo,
            player.reloading(),
            player.grenades
        );
        println!("[selftest] sound cues (own, others): {:?}", *cue_count);
        println!("[selftest] mode {} team scores {:?} my team {}", session.mode, session.team_scores, session.team_of(session.my_id));
        connection.send(&ClientMsg::EndMatch);
    }
}

/// The screenshots of `selftest_play` taken when something first happens.
#[derive(Default)]
struct Captured {
    marker: bool,
    countdown: bool,
    ally: bool,
    hurt: bool,
}

/// Tallies the sound cues of a self-test run: (own, others') per kind.
#[derive(Resource, Default, Debug)]
struct CueCount {
    gunshots: (u32, u32),
    hits: (u32, u32),
    kills: (u32, u32),
    footsteps: (u32, u32),
    landings: (u32, u32),
    reloads: (u32, u32),
    throws: (u32, u32),
    bounces: (u32, u32),
    explosions: (u32, u32),
    armor: (u32, u32),
    /// Countdown beeps and pings.
    signals: (u32, u32),
}

fn count_cues(mut cues: MessageReader<crate::audio::SoundCue>, mut count: ResMut<CueCount>) {
    use crate::audio::Sound;
    for cue in cues.read() {
        let slot = match cue.sound {
            Sound::Gunshot(_) => &mut count.gunshots,
            Sound::Hit | Sound::Headshot => &mut count.hits,
            Sound::Kill => &mut count.kills,
            Sound::Footstep => &mut count.footsteps,
            Sound::Land => &mut count.landings,
            Sound::Reload => &mut count.reloads,
            Sound::Pin | Sound::Throw => &mut count.throws,
            Sound::Bounce => &mut count.bounces,
            Sound::Explosion | Sound::Flashbang | Sound::SmokePop => &mut count.explosions,
            Sound::Ringing => &mut count.explosions,
            Sound::ArmorBreak | Sound::ArmorBroken | Sound::ArmorPickup => &mut count.armor,
            Sound::Countdown | Sound::Go | Sound::Ping => &mut count.signals,
        };
        if cue.at.is_none() {
            slot.0 += 1;
        } else {
            slot.1 += 1;
        }
    }
}
