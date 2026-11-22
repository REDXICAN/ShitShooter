//! Developer helpers: F12 screenshots, and a scripted self-test (`--selftest <dir>`)
//! that plays a short match against bots by itself and saves screenshots.

use std::{path::PathBuf, time::SystemTime};

use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};

use crate::{
    AppState, LaunchArgs,
    net::{Connection, MenuStatus, Session},
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
    if test.reached(2, 2.5) {
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
    mut marker_captured: Local<bool>,
) {
    if player.hit_marker > 0.0 && !*marker_captured {
        *marker_captured = true;
        test.capture(&mut commands, "hit.png");
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
    // The trigger is held, except with the pistol, which wants a click per shot.
    if (4.4..5.0).contains(&t) && (t * 4.0).fract() > 0.5 {
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
    // A grenade early on, a crouch that turns into a slide, and a reload by hand.
    hold(KeyCode::KeyG, (1.0..1.1).contains(&t));
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
        println!("[selftest] aim {:.2} ammo {} grenades {:.2}", player.aim, player.guns[0].ammo, player.grenades);
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

    if test.reached(2, 3.6) {
        test.capture(&mut commands, "play.png");
    }
    if test.reached(3, 6.0) {
        test.capture(&mut commands, "scores.png");
    }
    if test.reached(4, 7.5) {
        controls.captured = false;
    }
    if test.reached(5, 8.0) {
        test.capture(&mut commands, "pause.png");
    }
    // A screenshot is taken a frame or two after it is asked for, so the
    // settings open a moment later, not at once.
    if test.reached(6, 8.3) {
        settings_menu.open = true;
    }
    if test.reached(7, 8.8) {
        test.capture(&mut commands, "settings.png");
    }
    if test.reached(8, 9.2) {
        settings_menu.open = false;
    }
    if test.reached(9, 9.6) {
        let me = session.roster.iter().find(|p| p.id == session.my_id);
        println!(
            "[selftest] pos {:.1?} top speed {:.1} health {} me {:?} ping {:.1}ms",
            player.pos,
            test.top_speed,
            player.health,
            me.map(|p| (p.kills, p.deaths)),
            connection.ping_ms()
        );
        println!(
            "[selftest] ammo {} reloading {} grenades {:.2}",
            player.guns[0].ammo,
            player.reloading(),
            player.grenades
        );
        println!("[selftest] sound cues (own, others): {:?}", *cue_count);
        println!("[selftest] mode {} team scores {:?} my team {}", session.mode, session.team_scores, session.team_of(session.my_id));
        connection.send(&ClientMsg::EndMatch);
    }
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
            Sound::Throw => &mut count.throws,
            Sound::Bounce => &mut count.bounces,
            Sound::Explosion => &mut count.explosions,
        };
        if cue.at.is_none() {
            slot.0 += 1;
        } else {
            slot.1 += 1;
        }
    }
}
