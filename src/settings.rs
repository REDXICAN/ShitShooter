//! Player settings: mouse, field of view, volume, window and key bindings, plus the
//! name and address last typed into the menu. They are saved to a small text
//! file so they survive restarts, and edited in the settings panel that opens
//! from the main menu, the lobby and the pause menu.

use std::path::PathBuf;

use bevy::{
    audio::Volume,
    ecs::system::SystemParam,
    input::{
        InputSystems,
        mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    },
    input_focus::InputFocus,
    picking::hover::Hovered,
    prelude::*,
    ui_widgets::{Activate, Button},
    window::{PrimaryWindow, WindowMode},
};

use crate::{
    AppState, LaunchArgs,
    player::Controls,
    weapons::{
        GUN_COUNT, IRON_SIGHTS, PISTOL, RED_DOT, RIFLE, SCOPE, SIGHTS, SNIPER, SightId, WeaponId, next_gun, sight,
        sight_on, sights_for, takes_sights, weapon,
    },
    ui::{self, ACCENT, BUTTON, DIM, TEXT},
};

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        let args = app.world().resource::<LaunchArgs>();
        // The scripted self-test always runs with the defaults and saves nothing.
        let settings = if args.selftest.is_some() {
            Settings::default()
        } else {
            Settings::load()
        };
        // Set before the window is opened, so the game does not start in a
        // window and then jump to full screen.
        let window_mode = settings.window_mode();
        let world = app.world_mut();
        for mut window in world.query::<&mut Window>().iter_mut(world) {
            window.mode = window_mode;
        }
        app.insert_resource(settings)
            .init_resource::<SettingsMenu>()
            .init_resource::<WheelSteps>()
            .add_systems(PreUpdate, read_wheel.after(InputSystems))
            .add_systems(OnExit(AppState::Playing), close_menu)
            .add_systems(
                Update,
                (
                    escape_key.run_if(not(in_state(AppState::Playing))),
                    capture_bind,
                    fullscreen_key,
                    sync_panel,
                    update_panel,
                    apply_volume,
                    apply_display,
                )
                    .chain(),
            );
    }
}

// ---------------------------------------------------------------- bindings

/// Everything the player can do that has a key or mouse button.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Forward,
    Back,
    Left,
    Right,
    Jump,
    Crouch,
    Dash,
    Fire,
    Aim,
    Reload,
    Grenade,
    Primary,
    Secondary,
    Knife,
    NextWeapon,
    PreviousWeapon,
    Scores,
}

impl Action {
    pub const ALL: [Action; 17] = [
        Action::Forward,
        Action::Back,
        Action::Left,
        Action::Right,
        Action::Jump,
        Action::Crouch,
        Action::Dash,
        Action::Fire,
        Action::Aim,
        Action::Reload,
        Action::Grenade,
        Action::Primary,
        Action::Secondary,
        Action::Knife,
        Action::NextWeapon,
        Action::PreviousWeapon,
        Action::Scores,
    ];

    /// (name in the settings panel, name in the settings file, default binding)
    fn info(self) -> (&'static str, &'static str, Bind) {
        use Bind::{Key, Mouse, WheelDown, WheelUp};
        match self {
            Action::Forward => ("Move forward", "forward", Key(KeyCode::KeyW)),
            Action::Back => ("Move back", "back", Key(KeyCode::KeyS)),
            Action::Left => ("Move left", "left", Key(KeyCode::KeyA)),
            Action::Right => ("Move right", "right", Key(KeyCode::KeyD)),
            Action::Jump => ("Jump", "jump", Key(KeyCode::Space)),
            Action::Crouch => ("Crouch / slide", "crouch", Key(KeyCode::ControlLeft)),
            Action::Dash => ("Dash", "dash", Key(KeyCode::ShiftLeft)),
            Action::Fire => ("Fire", "fire", Mouse(MouseButton::Left)),
            Action::Aim => ("Aim down sights", "aim", Mouse(MouseButton::Right)),
            Action::Reload => ("Reload", "reload", Key(KeyCode::KeyR)),
            Action::Grenade => ("Grenade (hold: wheel)", "grenade", Key(KeyCode::KeyG)),
            Action::Primary => ("Primary gun", "primary", Key(KeyCode::Digit1)),
            Action::Secondary => ("Secondary gun", "secondary", Key(KeyCode::Digit2)),
            Action::Knife => ("Knife", "knife", Key(KeyCode::Digit3)),
            Action::NextWeapon => ("Next weapon", "next_weapon", WheelDown),
            Action::PreviousWeapon => ("Previous weapon", "previous_weapon", WheelUp),
            Action::Scores => ("Scoreboard", "scores", Key(KeyCode::Tab)),
        }
    }

    pub fn label(self) -> &'static str {
        self.info().0
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Bind {
    Key(KeyCode),
    Mouse(MouseButton),
    /// One notch of the scroll wheel: "pressed" for the frame it turns.
    WheelUp,
    WheelDown,
}

/// Keys that can be bound. Escape (pause) and F12 (screenshot) are reserved.
#[rustfmt::skip]
const BINDABLE_KEYS: &[KeyCode] = {
    use KeyCode::*;
    &[
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM,
        KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
        Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
        F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11,
        ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
        Space, Tab, Enter, Backspace, CapsLock,
        ShiftLeft, ShiftRight, ControlLeft, ControlRight, AltLeft, AltRight,
        Backquote, Minus, Equal, BracketLeft, BracketRight, Backslash,
        Semicolon, Quote, Comma, Period, Slash,
        Insert, Delete, Home, End, PageUp, PageDown,
        Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9,
        NumpadAdd, NumpadSubtract, NumpadMultiply, NumpadDivide, NumpadEnter, NumpadDecimal,
    ]
};

const BINDABLE_BUTTONS: &[(MouseButton, &str)] = &[
    (MouseButton::Left, "MouseLeft"),
    (MouseButton::Right, "MouseRight"),
    (MouseButton::Middle, "MouseMiddle"),
    (MouseButton::Back, "MouseBack"),
    (MouseButton::Forward, "MouseForward"),
];

impl Bind {
    /// The name used in the settings file, e.g. `KeyW`, `ShiftLeft`, `MouseRight`.
    fn file_name(self) -> String {
        match self {
            Bind::Key(key) => format!("{key:?}"),
            Bind::Mouse(button) => BINDABLE_BUTTONS
                .iter()
                .find(|(b, _)| *b == button)
                .map_or("MouseLeft", |(_, name)| name)
                .to_string(),
            Bind::WheelUp => "WheelUp".to_string(),
            Bind::WheelDown => "WheelDown".to_string(),
        }
    }

    fn parse(name: &str) -> Option<Bind> {
        match name {
            "WheelUp" => return Some(Bind::WheelUp),
            "WheelDown" => return Some(Bind::WheelDown),
            _ => {}
        }
        if let Some((button, _)) = BINDABLE_BUTTONS.iter().find(|(_, n)| *n == name) {
            return Some(Bind::Mouse(*button));
        }
        BINDABLE_KEYS
            .iter()
            .find(|key| format!("{key:?}") == name)
            .map(|key| Bind::Key(*key))
    }

    /// The name shown to the player, e.g. `W`, `LEFT SHIFT`, `MOUSE RIGHT`.
    pub fn display(self) -> String {
        let name = self.file_name();
        let spaced = |rest: &str, prefix: &str| format!("{prefix} {}", rest.to_uppercase());
        if let Some(rest) = name.strip_prefix("Mouse") {
            spaced(rest, "MOUSE")
        } else if let Some(rest) = name.strip_prefix("Wheel") {
            spaced(rest, "WHEEL")
        } else if let Some(rest) = name.strip_prefix("Key").or(name.strip_prefix("Digit")) {
            rest.to_string()
        } else if let Some(rest) = name.strip_prefix("Numpad") {
            spaced(rest, "NUM")
        } else if let Some(rest) = name.strip_prefix("Arrow") {
            spaced(rest, "ARROW")
        } else if let Some(rest) = name.strip_suffix("Left") {
            spaced(rest, "LEFT")
        } else if let Some(rest) = name.strip_suffix("Right") {
            spaced(rest, "RIGHT")
        } else {
            name.to_uppercase()
        }
    }
}

// ---------------------------------------------------------------- the settings

#[derive(Resource, Clone, PartialEq, Debug)]
pub struct Settings {
    /// Last name, join address and host port typed into the main menu.
    pub name: String,
    pub address: String,
    pub port: String,
    /// Mouse sensitivity: thousandths of a radian per pixel of mouse movement.
    pub sensitivity: f32,
    /// Extra sensitivity factor while aiming down sights.
    pub ads_sensitivity: f32,
    /// Vertical field of view in degrees.
    pub fov: f32,
    /// Master volume, 0 to 1.
    pub volume: f32,
    /// Borderless full screen, rather than a window.
    pub fullscreen: bool,
    /// The guns carried in the two slots. Never the same gun twice.
    pub primary: WeaponId,
    pub secondary: WeaponId,
    /// The sight picked for each gun, by weapon id. Guns without a sight
    /// mount always have their iron sights.
    pub sights: [SightId; GUN_COUNT as usize],
    binds: [Bind; Action::ALL.len()],
    /// False during the self-test, which must not touch the player's file.
    persist: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            name: "Player".into(),
            address: "127.0.0.1".into(),
            port: crate::protocol::DEFAULT_PORT.to_string(),
            sensitivity: 2.2,
            ads_sensitivity: 1.0,
            fov: 75.0,
            volume: 0.8,
            fullscreen: false,
            primary: RIFLE,
            secondary: PISTOL,
            sights: default_sights(),
            binds: Action::ALL.map(|action| action.info().2),
            persist: false,
        }
    }
}

/// A number in the settings panel that is changed with - and + buttons.
#[derive(Clone, Copy, PartialEq)]
enum Dial {
    Sensitivity,
    AdsSensitivity,
    Fov,
    Volume,
}

impl Dial {
    const ALL: [Dial; 4] = [Dial::Sensitivity, Dial::AdsSensitivity, Dial::Fov, Dial::Volume];

    /// (label, step, minimum, maximum)
    fn info(self) -> (&'static str, f32, f32, f32) {
        match self {
            Dial::Sensitivity => ("Mouse sensitivity", 0.1, 0.2, 12.0),
            Dial::AdsSensitivity => ("Aim sensitivity", 0.05, 0.2, 2.0),
            Dial::Fov => ("Field of view", 5.0, 50.0, 100.0),
            Dial::Volume => ("Volume", 0.05, 0.0, 1.0),
        }
    }
}

impl Settings {
    pub fn bind(&self, action: Action) -> Bind {
        self.binds[action as usize]
    }

    /// Binds `action`. If another action already used that key, the two swap,
    /// so nothing is ever left without a binding.
    fn set_bind(&mut self, action: Action, bind: Bind) {
        let previous = self.binds[action as usize];
        if let Some(other) = self.binds.iter_mut().find(|b| **b == bind) {
            *other = previous;
        }
        self.binds[action as usize] = bind;
    }

    fn dial(&mut self, dial: Dial) -> &mut f32 {
        match dial {
            Dial::Sensitivity => &mut self.sensitivity,
            Dial::AdsSensitivity => &mut self.ads_sensitivity,
            Dial::Fov => &mut self.fov,
            Dial::Volume => &mut self.volume,
        }
    }

    fn dial_text(&self, dial: Dial) -> String {
        match dial {
            Dial::Sensitivity => format!("{:.1}", self.sensitivity),
            Dial::AdsSensitivity => format!("x {:.2}", self.ads_sensitivity),
            Dial::Fov => format!("{:.0}", self.fov),
            Dial::Volume => format!("{:.0}%", self.volume * 100.0),
        }
    }

    /// Puts every number back inside its allowed range.
    fn clamp(&mut self) {
        for dial in Dial::ALL {
            let (_, _, min, max) = dial.info();
            let value = self.dial(dial);
            *value = if value.is_finite() { value.clamp(min, max) } else { min };
        }
    }

    fn window_mode(&self) -> WindowMode {
        if self.fullscreen {
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        } else {
            WindowMode::Windowed
        }
    }

    /// Mouse, sound and keys go back to their defaults; who you are, what you
    /// carry and the window stay as they are.
    fn reset_controls(&mut self) {
        let defaults = Settings::default();
        *self = Settings {
            name: std::mem::take(&mut self.name),
            address: std::mem::take(&mut self.address),
            port: std::mem::take(&mut self.port),
            fullscreen: self.fullscreen,
            primary: self.primary,
            secondary: self.secondary,
            sights: self.sights,
            persist: self.persist,
            ..defaults
        };
    }

    /// Puts the next gun in a slot (0 = primary, 1 = secondary).
    pub fn cycle_gun(&mut self, slot: usize) {
        if slot == 0 {
            self.primary = next_gun(self.primary, self.secondary);
        } else {
            self.secondary = next_gun(self.secondary, self.primary);
        }
    }

    /// The gun in a slot (0 = primary, 1 = secondary).
    pub fn gun(&self, slot: usize) -> WeaponId {
        if slot == 0 { self.primary } else { self.secondary }
    }

    /// The sight a gun is handed out with.
    pub fn sight_for(&self, gun: WeaponId) -> SightId {
        sight_on(gun, self.sights.get(gun as usize).copied().unwrap_or(IRON_SIGHTS))
    }

    /// Puts the next sight the gun in a slot can take on it: its iron sights,
    /// then each of `weapons::sights_for` in turn.
    pub fn cycle_sight(&mut self, slot: usize) {
        let gun = self.gun(slot);
        let current = self.sight_for(gun);
        let choices: Vec<SightId> = std::iter::once(IRON_SIGHTS).chain(sights_for(gun).iter().copied()).collect();
        let next = choices.iter().position(|&s| s == current).map_or(0, |i| (i + 1) % choices.len());
        if let Some(sight) = self.sights.get_mut(gun as usize) {
            *sight = choices[next];
        }
    }

    /// Reads the settings file. Anything missing or unreadable keeps its default.
    fn load() -> Settings {
        let text = settings_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_default();
        Settings {
            persist: true,
            ..Settings::parse(&text)
        }
    }

    pub fn save(&self) {
        if !self.persist {
            return;
        }
        let Some(path) = settings_path() else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(err) = std::fs::write(&path, self.to_text()) {
            warn!("could not save settings to {}: {err}", path.display());
        }
    }

    fn parse(text: &str) -> Settings {
        let mut settings = Settings::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            let number = value.parse::<f32>().ok();
            match key {
                "name" => settings.name = value.to_string(),
                "address" => settings.address = value.to_string(),
                "port" => settings.port = value.to_string(),
                "sensitivity" => settings.sensitivity = number.unwrap_or(settings.sensitivity),
                "ads_sensitivity" => settings.ads_sensitivity = number.unwrap_or(settings.ads_sensitivity),
                "fov" => settings.fov = number.unwrap_or(settings.fov),
                "volume" => settings.volume = number.unwrap_or(settings.volume),
                "fullscreen" => settings.fullscreen = value.parse().unwrap_or(settings.fullscreen),
                "primary" => settings.primary = gun_named(value).unwrap_or(settings.primary),
                "secondary" => settings.secondary = gun_named(value).unwrap_or(settings.secondary),
                _ if key.starts_with("sight.") => {
                    let gun = gun_named(&key["sight.".len()..]);
                    let picked = SIGHTS.iter().position(|s| s.name.eq_ignore_ascii_case(value));
                    if let (Some(gun), Some(picked)) = (gun, picked) {
                        settings.sights[gun as usize] = picked as SightId;
                    }
                }
                _ => {
                    let action = key
                        .strip_prefix("bind.")
                        .and_then(|name| Action::ALL.into_iter().find(|a| a.info().1 == name));
                    // Swapped in, so that a key saved for one action never ends
                    // up doubling the default of an action added since.
                    if let (Some(action), Some(bind)) = (action, Bind::parse(value)) {
                        settings.set_bind(action, bind);
                    }
                }
            }
        }
        settings.clamp();
        if settings.secondary == settings.primary {
            settings.secondary = next_gun(settings.secondary, settings.primary);
        }
        settings
    }

    fn to_text(&self) -> String {
        let mut text = format!(
            "name = {}\naddress = {}\nport = {}\nsensitivity = {}\nads_sensitivity = {}\nfov = {}\nvolume = {}\n",
            self.name, self.address, self.port, self.sensitivity, self.ads_sensitivity, self.fov, self.volume
        );
        text.push_str(&format!(
            "fullscreen = {}\nprimary = {}\nsecondary = {}\n",
            self.fullscreen,
            weapon(self.primary).name,
            weapon(self.secondary).name
        ));
        for gun in (0..GUN_COUNT).filter(|gun| takes_sights(*gun)) {
            text.push_str(&format!("sight.{} = {}
", weapon(gun).name, sight(self.sights[gun as usize]).name));
        }
        for action in Action::ALL {
            text.push_str(&format!("bind.{} = {}\n", action.info().1, self.bind(action).file_name()));
        }
        text
    }
}

/// The rifle comes with a red dot and the sniper with a scope; every other
/// gun with its iron sights.
fn default_sights() -> [SightId; GUN_COUNT as usize] {
    let mut sights = [IRON_SIGHTS; GUN_COUNT as usize];
    sights[RIFLE as usize] = RED_DOT;
    sights[SNIPER as usize] = SCOPE;
    sights
}

fn gun_named(name: &str) -> Option<WeaponId> {
    (0..GUN_COUNT).find(|id| weapon(*id).name.eq_ignore_ascii_case(name))
}

/// `%APPDATA%\ShitShooter\settings.txt` (the same for every copy of the exe, so
/// a new build keeps your settings), or next to the exe if that is not available.
fn settings_path() -> Option<PathBuf> {
    let config_dir = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    };
    config_dir
        .map(|dir| dir.join("ShitShooter"))
        .or_else(|| std::env::current_exe().ok()?.parent().map(PathBuf::from))
        .map(|dir| dir.join("settings.txt"))
}

// ---------------------------------------------------------------- reading input

/// The player's bound keys and buttons. Everything reads as "not pressed"
/// while the game is paused.
#[derive(SystemParam)]
pub struct ActionInput<'w> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    mouse: Res<'w, ButtonInput<MouseButton>>,
    wheel: Res<'w, WheelSteps>,
    settings: Res<'w, Settings>,
    controls: Res<'w, Controls>,
}

impl ActionInput<'_> {
    pub fn pressed(&self, action: Action) -> bool {
        self.controls.captured
            && match self.settings.bind(action) {
                Bind::Key(key) => self.keys.pressed(key),
                Bind::Mouse(button) => self.mouse.pressed(button),
                wheel => self.wheel.step == Some(wheel),
            }
    }

    pub fn just_pressed(&self, action: Action) -> bool {
        self.controls.captured
            && match self.settings.bind(action) {
                Bind::Key(key) => self.keys.just_pressed(key),
                Bind::Mouse(button) => self.mouse.just_pressed(button),
                wheel => self.wheel.step == Some(wheel),
            }
    }
}

/// The scroll wheel turned into notches: `step` is the one this frame, if any.
#[derive(Resource, Default)]
pub struct WheelSteps {
    step: Option<Bind>,
    /// Scrolled since the last notch. Smooth scrolling (touchpads, free-spinning
    /// wheels) comes in small amounts that add up to a notch.
    owed: f32,
}

/// At most one notch a frame, so a hard flick does not race through every weapon.
fn read_wheel(scroll: Res<AccumulatedMouseScroll>, mut wheel: ResMut<WheelSteps>) {
    let (delta, notch) = match scroll.unit {
        MouseScrollUnit::Line => (scroll.delta.y, 1.0),
        MouseScrollUnit::Pixel => (scroll.delta.y, 40.0),
    };
    // Turning the other way starts afresh.
    if delta * wheel.owed < 0.0 {
        wheel.owed = 0.0;
    }
    wheel.owed += delta;
    wheel.step = None;
    if wheel.owed.abs() >= notch * 0.99 {
        wheel.step = Some(if wheel.owed > 0.0 { Bind::WheelUp } else { Bind::WheelDown });
        wheel.owed = (wheel.owed.abs() - notch).clamp(0.0, notch) * wheel.owed.signum();
    }
}

// ---------------------------------------------------------------- the panel

/// Whether the settings panel is open, and which binding is waiting for a key.
#[derive(Resource, Default)]
pub struct SettingsMenu {
    pub open: bool,
    capturing: Option<Capture>,
}

struct Capture {
    action: Action,
    /// Set once every key and button has been let go, so the click that
    /// started the capture is not itself taken as the new binding.
    armed: bool,
}

impl SettingsMenu {
    /// What Escape does while the panel is open: cancel a pending rebind, or close.
    pub fn back(&mut self) {
        if self.capturing.take().is_none() {
            self.open = false;
        }
    }
}

#[derive(Component)]
struct SettingsPanel;
#[derive(Component)]
struct DialText(Dial);
#[derive(Component)]
struct BindText(Action);
/// The text of the button that switches between a window and full screen.
#[derive(Component)]
struct DisplayText;

/// Observer for any button that opens the settings panel.
pub fn on_open_settings(_: On<Activate>, mut menu: ResMut<SettingsMenu>) {
    menu.open = true;
}

fn close_menu(mut menu: ResMut<SettingsMenu>) {
    menu.open = false;
    menu.capturing = None;
}

fn escape_key(keys: Res<ButtonInput<KeyCode>>, mut menu: ResMut<SettingsMenu>) {
    if menu.open && keys.just_pressed(KeyCode::Escape) {
        menu.back();
    }
}

/// While a binding waits for input, the next key or mouse button becomes it.
fn capture_bind(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    wheel: Res<WheelSteps>,
    mut menu: ResMut<SettingsMenu>,
    mut settings: ResMut<Settings>,
) {
    let Some(capture) = &mut menu.capturing else {
        return;
    };
    if !capture.armed {
        capture.armed = keys.get_pressed().next().is_none() && mouse.get_pressed().next().is_none();
        return;
    }
    let key = keys
        .get_just_pressed()
        .find(|key| BINDABLE_KEYS.contains(key))
        .map(|key| Bind::Key(*key));
    let button = mouse
        .get_just_pressed()
        .find(|button| BINDABLE_BUTTONS.iter().any(|(b, _)| b == *button))
        .map(|button| Bind::Mouse(*button));
    if let Some(bind) = key.or(button).or(wheel.step) {
        settings.set_bind(capture.action, bind);
        menu.capturing = None;
    }
}

/// Spawns the panel when it is opened; removes it and saves when it is closed.
fn sync_panel(
    mut commands: Commands,
    mut menu: ResMut<SettingsMenu>,
    settings: Res<Settings>,
    mut focus: ResMut<InputFocus>,
    panel: Query<Entity, With<SettingsPanel>>,
) {
    match (menu.open, panel.iter().next()) {
        (true, None) => {
            // Otherwise a menu text field behind the panel keeps taking the typing.
            focus.clear();
            spawn_panel(&mut commands);
        }
        (false, Some(entity)) => {
            commands.entity(entity).despawn();
            menu.capturing = None;
            settings.save();
        }
        _ => {}
    }
}

fn spawn_panel(commands: &mut Commands) {
    let row = || Node {
        width: px(350),
        ..ui::spread_row()
    };
    // A small button with fixed width.
    let key_button = |width: i32| {
        (
            Button,
            Hovered::default(),
            Node {
                width: px(width),
                padding: UiRect::axes(px(8), px(5)),
                justify_content: JustifyContent::Center,
                border_radius: BorderRadius::all(px(5)),
                ..default()
            },
            BackgroundColor(BUTTON),
        )
    };

    commands
        .spawn((
            SettingsPanel,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            // Above everything, the pause menu included, and dark enough to hide it.
            GlobalZIndex(20),
            BackgroundColor(Color::srgba(0.01, 0.012, 0.02, 0.98)),
        ))
        .with_children(|screen| {
            screen.spawn(ui::panel()).with_children(|panel| {
                panel.spawn(ui::label("SETTINGS", 36.0, ACCENT));

                panel
                    .spawn(Node {
                        column_gap: px(14),
                        align_self: AlignSelf::Stretch,
                        justify_content: JustifyContent::Center,
                        ..default()
                    })
                    .with_children(|cards| {
                        cards.spawn(ui::card()).with_children(|dials| {
                            dials.spawn(ui::section("Mouse and sound"));
                            for dial in Dial::ALL {
                                dials.spawn(row()).with_children(|row| {
                                    row.spawn(ui::label(dial.info().0, 17.0, DIM));
                                    row.spawn(Node {
                                        align_items: AlignItems::Center,
                                        column_gap: px(8),
                                        ..default()
                                    })
                                    .with_children(|controls| {
                                        controls
                                            .spawn((
                                                key_button(34),
                                                children![(ui::label("-", 18.0, TEXT), Pickable::IGNORE)],
                                            ))
                                            .observe(turn_dial(dial, -1.0));
                                        controls.spawn((
                                            ui::label("", 17.0, TEXT),
                                            DialText(dial),
                                            TextLayout::justify(Justify::Center),
                                            Node {
                                                width: px(70),
                                                ..default()
                                            },
                                        ));
                                        controls
                                            .spawn((
                                                key_button(34),
                                                children![(ui::label("+", 18.0, TEXT), Pickable::IGNORE)],
                                            ))
                                            .observe(turn_dial(dial, 1.0));
                                    });
                                });
                            }
                        });

                        cards.spawn(ui::card()).with_children(|display| {
                            display.spawn(ui::section("Display"));
                            display.spawn(row()).with_children(|row| {
                                row.spawn(ui::label("Window", 17.0, DIM));
                                row.spawn((
                                    key_button(230),
                                    children![(ui::label("", 16.0, TEXT), Pickable::IGNORE, DisplayText)],
                                ))
                                .observe(|_: On<Activate>, mut settings: ResMut<Settings>| {
                                    settings.fullscreen = !settings.fullscreen;
                                });
                            });
                            display.spawn(ui::label("Alt + Enter switches at any time.", 14.0, DIM));
                        });
                    });

                panel.spawn(ui::card()).with_children(|card| {
                    card.spawn(ui::spread_row()).with_children(|heading| {
                        heading.spawn(ui::section("Controls"));
                        heading.spawn(ui::label(
                            "Click a control, then press the key or mouse button (or scroll) for it.",
                            14.0,
                            DIM,
                        ));
                    });
                    card.spawn(Node {
                        column_gap: px(28),
                        ..default()
                    })
                    .with_children(|columns| {
                        for third in Action::ALL.chunks(Action::ALL.len().div_ceil(3)) {
                            columns
                                .spawn(Node {
                                    flex_direction: FlexDirection::Column,
                                    row_gap: px(6),
                                    ..default()
                                })
                                .with_children(|binds| {
                                    for action in third.iter().copied() {
                                        binds
                                            .spawn(Node {
                                                width: px(320),
                                                ..row()
                                            })
                                            .with_children(|row| {
                                                row.spawn(ui::label(action.label(), 17.0, DIM));
                                                row.spawn((
                                                    key_button(140),
                                                    children![(
                                                        ui::label("", 16.0, TEXT),
                                                        Pickable::IGNORE,
                                                        BindText(action)
                                                    )],
                                                ))
                                                .observe(
                                                    move |_: On<Activate>, mut menu: ResMut<SettingsMenu>| {
                                                        menu.capturing = Some(Capture {
                                                            action,
                                                            armed: false,
                                                        });
                                                    },
                                                );
                                            });
                                    }
                                });
                        }
                    });
                });

                panel.spawn(ui::button_row()).with_children(|buttons| {
                    buttons
                        .spawn(ui::button("Reset to defaults"))
                        .observe(|_: On<Activate>, mut settings: ResMut<Settings>| settings.reset_controls());
                    buttons
                        .spawn(ui::primary_button("Done"))
                        .observe(|_: On<Activate>, mut menu: ResMut<SettingsMenu>| menu.open = false);
                });
            });
        });
}

/// An observer that moves `dial` one step up or down when its button is clicked.
fn turn_dial(dial: Dial, direction: f32) -> impl Fn(On<Activate>, ResMut<Settings>) {
    move |_, mut settings| {
        let (_, step, ..) = dial.info();
        let value = settings.dial(dial);
        // Snap to whole steps, so repeated clicks do not drift (0.1 is not exact in binary).
        *value = ((*value / step).round() + direction) * step;
        settings.clamp();
    }
}

fn update_panel(
    settings: Res<Settings>,
    menu: Res<SettingsMenu>,
    mut dials: Query<(&DialText, &mut Text), Without<BindText>>,
    mut binds: Query<(&BindText, &mut Text, &mut TextColor), Without<DialText>>,
    mut display: Query<&mut Text, (With<DisplayText>, Without<DialText>, Without<BindText>)>,
) {
    for mut text in &mut display {
        let shown = if settings.fullscreen { "Borderless fullscreen" } else { "Windowed" };
        ui::set_text(&mut text, shown.to_string());
    }
    for (dial, mut text) in &mut dials {
        ui::set_text(&mut text, settings.dial_text(dial.0));
    }
    let capturing = menu.capturing.as_ref().map(|capture| capture.action);
    for (bind, mut text, mut color) in &mut binds {
        let waiting = capturing == Some(bind.0);
        let shown = if waiting {
            "press a key...".to_string()
        } else {
            settings.bind(bind.0).display()
        };
        ui::set_text(&mut text, shown);
        let wanted = if waiting { ACCENT } else { TEXT };
        if color.0 != wanted {
            color.0 = wanted;
        }
    }
}

/// Alt + Enter switches between a window and full screen, wherever you are.
fn fullscreen_key(keys: Res<ButtonInput<KeyCode>>, menu: Res<SettingsMenu>, mut settings: ResMut<Settings>) {
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    // Not while a binding waits for a key: that key press is for the binding.
    if alt && keys.just_pressed(KeyCode::Enter) && menu.capturing.is_none() {
        settings.fullscreen = !settings.fullscreen;
        settings.save();
    }
}

fn apply_display(settings: Res<Settings>, mut window: Single<&mut Window, With<PrimaryWindow>>) {
    let wanted = settings.window_mode();
    if window.mode != wanted {
        window.mode = wanted;
    }
}

fn apply_volume(settings: Res<Settings>, args: Res<LaunchArgs>, mut global: ResMut<GlobalVolume>) {
    // The self-test runs muted.
    if settings.is_changed() && args.selftest.is_none() {
        // Squared, so the steps sound evenly spaced.
        global.volume = Volume::Linear(settings.volume * settings.volume);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapons::REFLEX;

    #[test]
    fn settings_survive_a_save_and_load() {
        let mut settings = Settings {
            name: "Some = Name".into(),
            address: "example.org:7000".into(),
            sensitivity: 3.4,
            fov: 90.0,
            volume: 0.35,
            fullscreen: true,
            primary: 3,
            secondary: 1,
            sights: [IRON_SIGHTS, RED_DOT, IRON_SIGHTS, RED_DOT, RED_DOT],
            ..default()
        };
        settings.set_bind(Action::Grenade, Bind::Mouse(MouseButton::Middle));
        settings.set_bind(Action::Crouch, Bind::Key(KeyCode::KeyC));
        assert_eq!(Settings::parse(&settings.to_text()), settings);
    }

    #[test]
    fn sights_go_only_on_guns_with_a_mount() {
        let shotgun = gun_named("Shotgun").unwrap();
        let smg = gun_named("SMG").unwrap();
        let mut settings = Settings::parse("primary = Shotgun\nsecondary = SMG\nsight.Shotgun = Red dot\nsight.SMG = red DOT\n");
        assert_eq!(settings.sight_for(shotgun), IRON_SIGHTS);
        assert_eq!(settings.sight_for(smg), RED_DOT);
        assert_eq!(settings.sight_for(PISTOL), IRON_SIGHTS);
        assert_eq!(settings.sight_for(RIFLE), RED_DOT, "the rifle's default");
        settings.cycle_sight(0);
        assert_eq!(settings.sight_for(shotgun), IRON_SIGHTS);
        settings.cycle_sight(1);
        assert_eq!(settings.sight_for(smg), REFLEX);
        settings.cycle_sight(1);
        assert_eq!(settings.sight_for(smg), IRON_SIGHTS, "cycling wraps round");
    }

    #[test]
    fn only_the_sniper_takes_the_scope() {
        let mut settings = Settings::parse("primary = Sniper
secondary = Pistol
sight.Pistol = Scope
");
        assert_eq!(settings.sight_for(PISTOL), IRON_SIGHTS);
        assert_eq!(settings.sight_for(SNIPER), SCOPE, "the sniper's default");
        let mut seen = Vec::new();
        for _ in 0..4 {
            settings.cycle_sight(0);
            seen.push(settings.sight_for(SNIPER));
        }
        assert_eq!(seen, [IRON_SIGHTS, RED_DOT, REFLEX, SCOPE]);
        settings.cycle_sight(1);
        assert_eq!(settings.sight_for(PISTOL), RED_DOT);
        settings.cycle_sight(1);
        assert_eq!(settings.sight_for(PISTOL), REFLEX);
        settings.cycle_sight(1);
        assert_eq!(settings.sight_for(PISTOL), IRON_SIGHTS, "the pistol skips the scope");
    }

    #[test]
    fn a_damaged_file_falls_back_to_defaults() {
        let parsed = Settings::parse("sensitivity = lots\nfov = 9000\nbind.jump = Banana\nnonsense\n");
        assert_eq!(parsed.sensitivity, Settings::default().sensitivity);
        assert_eq!(parsed.fov, 100.0);
        assert_eq!(parsed.bind(Action::Jump), Bind::Key(KeyCode::Space));
    }

    #[test]
    fn every_binding_has_a_name_that_reads_back() {
        let keys = BINDABLE_KEYS.iter().map(|key| Bind::Key(*key));
        let buttons = BINDABLE_BUTTONS.iter().map(|(button, _)| Bind::Mouse(*button));
        let wheel = [Bind::WheelUp, Bind::WheelDown];
        for bind in keys.chain(buttons).chain(wheel) {
            assert_eq!(Bind::parse(&bind.file_name()), Some(bind));
            assert!(!bind.display().is_empty());
        }
        assert_eq!(Bind::Key(KeyCode::ShiftLeft).display(), "LEFT SHIFT");
        assert_eq!(Bind::Key(KeyCode::KeyW).display(), "W");
        assert_eq!(Bind::Mouse(MouseButton::Right).display(), "MOUSE RIGHT");
        assert_eq!(Bind::WheelDown.display(), "WHEEL DOWN");
    }

    #[test]
    fn the_defaults_bind_every_action_to_something_different() {
        let settings = Settings::default();
        for (i, a) in Action::ALL.iter().enumerate() {
            for b in &Action::ALL[i + 1..] {
                assert_ne!(settings.bind(*a), settings.bind(*b), "{a:?} and {b:?}");
            }
        }
    }

    #[test]
    fn a_saved_key_never_ends_up_bound_twice() {
        // Crouch saved on the key that is the grenade's default.
        let parsed = Settings::parse("bind.crouch = KeyG\n");
        assert_eq!(parsed.bind(Action::Crouch), Bind::Key(KeyCode::KeyG));
        assert_ne!(parsed.bind(Action::Grenade), Bind::Key(KeyCode::KeyG));
    }

    #[test]
    fn rebinding_to_a_used_key_swaps() {
        let mut settings = Settings::default();
        settings.set_bind(Action::Reload, Bind::Key(KeyCode::KeyG));
        assert_eq!(settings.bind(Action::Reload), Bind::Key(KeyCode::KeyG));
        assert_eq!(settings.bind(Action::Grenade), Bind::Key(KeyCode::KeyR));
    }
}
