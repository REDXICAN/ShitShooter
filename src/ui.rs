//! All the 2D interface: main menu, lobby, in-game HUD, scoreboard and pause menu.

use bevy::{
    input_focus::tab_navigation::{TabGroup, TabIndex, TabNavigationPlugin},
    picking::hover::Hovered,
    prelude::*,
    text::{EditableText, TextCursorStyle},
    ui::Pressed,
    ui_widgets::{Activate, Button, TextInput, observe},
};

use crate::{
    AppState, LaunchArgs,
    combat::{HIT_MARKER_TIME, KillFeed, Notices},
    map::MAP_NAMES,
    weapons::{KNIFE, weapon},
    net::{self, Connection, MenuStatus, Session},
    player::{Controls, DASH_COOLDOWN, LocalPlayer},
    protocol::*,
    server::ServerSettings,
    settings::{Action, ActionInput, Settings, on_open_settings},
    world::GameMap,
};

const PANEL: Color = Color::srgba(0.05, 0.06, 0.09, 0.88);
pub const BUTTON: Color = Color::srgb(0.16, 0.18, 0.24);
const BUTTON_HOVER: Color = Color::srgb(0.25, 0.29, 0.40);
const BUTTON_PRESS: Color = Color::srgb(0.95, 0.55, 0.15);
const FIELD: Color = Color::srgb(0.10, 0.11, 0.15);
pub const TEXT: Color = Color::srgb(0.92, 0.93, 0.96);
pub const DIM: Color = Color::srgb(0.60, 0.63, 0.70);
pub const ACCENT: Color = Color::srgb(1.0, 0.62, 0.2);
const DANGER: Color = Color::srgb(1.0, 0.35, 0.3);

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        let args = app.world().resource::<LaunchArgs>().clone();
        // What was typed last time, unless the command line says otherwise.
        let saved = app.world().resource::<Settings>().clone();
        app.add_plugins(TabNavigationPlugin)
            .insert_resource(MenuPrefs {
                name: args.name.clone().unwrap_or(saved.name),
                address: args.join.clone().unwrap_or(saved.address),
                port: if args.server.port == DEFAULT_PORT {
                    saved.port
                } else {
                    args.server.port.to_string()
                },
                password: args.server.password.clone(),
            })
            .add_systems(Startup, auto_launch)
            .add_systems(OnEnter(AppState::Menu), spawn_menu)
            .add_systems(OnEnter(AppState::Connecting), spawn_connecting)
            .add_systems(OnEnter(AppState::Lobby), spawn_lobby)
            .add_systems(OnEnter(AppState::Playing), spawn_hud)
            .add_systems(
                Update,
                (
                    style_buttons,
                    update_menu_status.run_if(in_state(AppState::Menu)),
                    update_lobby.run_if(in_state(AppState::Lobby)),
                    update_loadout_buttons,
                    (update_hud, update_overlays).run_if(in_state(AppState::Playing)),
                ),
            );
    }
}

/// What was last typed into the menu, so it survives leaving and coming back.
#[derive(Resource, Clone)]
struct MenuPrefs {
    name: String,
    address: String,
    port: String,
    password: String,
}

#[derive(Component, Clone, Copy, PartialEq)]
enum Field {
    Name,
    Address,
    Port,
    Password,
}

// ---------------------------------------------------------------- building blocks

pub fn label(text: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

pub fn button(text: &str) -> impl Bundle {
    (
        Button,
        Hovered::default(),
        Node {
            padding: UiRect::axes(px(18), px(9)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(px(6)),
            ..default()
        },
        BackgroundColor(BUTTON),
        children![(label(text, 20.0, TEXT), Pickable::IGNORE)],
    )
}

fn text_field(field: Field, initial: &str, max_chars: usize, tab: i32) -> impl Bundle {
    (
        field,
        TextInput,
        EditableText {
            visible_width: Some(22.0),
            max_characters: Some(max_chars),
            ..EditableText::new(initial)
        },
        TextLayout::no_wrap(),
        TextFont {
            font_size: FontSize::Px(20.0),
            ..default()
        },
        TextColor(TEXT),
        TextCursorStyle::default(),
        TabIndex(tab),
        Node {
            border: UiRect::all(px(2)),
            padding: UiRect::axes(px(8), px(5)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BorderColor::all(Color::srgb(0.3, 0.33, 0.42)),
        BackgroundColor(FIELD),
    )
}

fn field_row(caption: &str, field: Field, initial: &str, max_chars: usize, tab: i32) -> impl Bundle {
    (
        Node {
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            column_gap: px(16),
            width: percent(100),
            ..default()
        },
        children![
            label(caption, 18.0, DIM),
            text_field(field, initial, max_chars, tab)
        ],
    )
}

/// Full-screen container that centres its children.
fn screen(state: AppState) -> impl Bundle {
    (
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        DespawnOnExit(state),
    )
}

pub fn panel() -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(12),
            padding: UiRect::all(px(28)),
            border_radius: BorderRadius::all(px(10)),
            ..default()
        },
        BackgroundColor(PANEL),
    )
}

pub fn button_row() -> Node {
    Node {
        column_gap: px(12),
        margin: UiRect::top(px(8)),
        ..default()
    }
}

fn style_buttons(mut buttons: Query<(&Hovered, Has<Pressed>, &mut BackgroundColor), With<Button>>) {
    for (hovered, pressed, mut background) in &mut buttons {
        let color = match (hovered.get(), pressed) {
            (_, true) => BUTTON_PRESS,
            (true, false) => BUTTON_HOVER,
            (false, false) => BUTTON,
        };
        if background.0 != color {
            background.0 = color;
        }
    }
}

pub fn set_text(text: &mut Mut<Text>, value: String) {
    if text.0 != value {
        text.0 = value;
    }
}

fn set_shown(node: &mut Mut<Node>, shown: bool) {
    let display = if shown { Display::Flex } else { Display::None };
    if node.display != display {
        node.display = display;
    }
}

// ---------------------------------------------------------------- main menu

#[derive(Component)]
struct MenuStatusText;

fn spawn_menu(mut commands: Commands, prefs: Res<MenuPrefs>) {
    commands.spawn((
        screen(AppState::Menu),
        TabGroup::new(0),
        children![(
            panel(),
            children![
                label("SHITSHOOTER", 54.0, ACCENT),
                label("fast free-for-all, hosted by you", 18.0, DIM),
                (
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(8),
                        margin: UiRect::vertical(px(10)),
                        ..default()
                    },
                    children![
                        field_row("Name", Field::Name, &prefs.name, 16, 0),
                        field_row("Join address", Field::Address, &prefs.address, 64, 1),
                        field_row("Host port", Field::Port, &prefs.port, 5, 2),
                        field_row("Password (optional)", Field::Password, &prefs.password, 32, 3),
                    ],
                ),
                (
                    button_row(),
                    children![
                        (button("Host lobby"), observe(on_host)),
                        (button("Join lobby"), observe(on_join)),
                        (button("Settings"), observe(on_open_settings)),
                        (button("Quit"), observe(on_quit)),
                    ],
                ),
                (label("", 16.0, DANGER), MenuStatusText),
            ],
        )],
    ));
}

fn update_menu_status(status: Res<MenuStatus>, mut text: Single<&mut Text, With<MenuStatusText>>) {
    if text.0 != status.0 {
        text.0 = status.0.clone();
    }
}

/// Takes what is in the text fields, and remembers it for the next time the
/// game is started (all but the password, which is not written to disk).
fn read_fields(fields: &Query<(&Field, &EditableText)>, prefs: &mut MenuPrefs, settings: &mut Settings) {
    for (field, text) in fields {
        let value = text.value().to_string().trim().to_string();
        match field {
            Field::Name => prefs.name = value,
            Field::Address => prefs.address = value,
            Field::Port => prefs.port = value,
            Field::Password => prefs.password = value,
        }
    }
    settings.name = prefs.name.clone();
    settings.address = prefs.address.clone();
    settings.port = prefs.port.clone();
    settings.save();
}

fn start_host(
    commands: &mut Commands,
    prefs: &MenuPrefs,
    args: &LaunchArgs,
    status: &mut MenuStatus,
    next_state: &mut NextState<AppState>,
) {
    let Ok(port) = prefs.port.parse::<u16>() else {
        status.0 = format!("'{}' is not a valid port.", prefs.port);
        return;
    };
    let settings = ServerSettings {
        port,
        password: prefs.password.clone(),
        ..args.server.clone()
    };
    finish_connect(net::host(commands, settings, &prefs.name), status, next_state);
}

fn start_join(
    commands: &mut Commands,
    prefs: &MenuPrefs,
    status: &mut MenuStatus,
    next_state: &mut NextState<AppState>,
) {
    let result = net::join(commands, &prefs.address, &prefs.password, &prefs.name);
    finish_connect(result, status, next_state);
}

fn finish_connect(result: Result<(), String>, status: &mut MenuStatus, next_state: &mut NextState<AppState>) {
    match result {
        Ok(()) => {
            status.0.clear();
            next_state.set(AppState::Connecting);
        }
        Err(message) => status.0 = message,
    }
}

fn on_host(
    _: On<Activate>,
    mut commands: Commands,
    fields: Query<(&Field, &EditableText)>,
    args: Res<LaunchArgs>,
    mut prefs: ResMut<MenuPrefs>,
    mut settings: ResMut<Settings>,
    mut status: ResMut<MenuStatus>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    read_fields(&fields, &mut prefs, &mut settings);
    start_host(&mut commands, &prefs, &args, &mut status, &mut next_state);
}

fn on_join(
    _: On<Activate>,
    mut commands: Commands,
    fields: Query<(&Field, &EditableText)>,
    mut prefs: ResMut<MenuPrefs>,
    mut settings: ResMut<Settings>,
    mut status: ResMut<MenuStatus>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    read_fields(&fields, &mut prefs, &mut settings);
    start_join(&mut commands, &prefs, &mut status, &mut next_state);
}

fn on_quit(_: On<Activate>, mut exit: MessageWriter<AppExit>) {
    exit.write(AppExit::Success);
}

/// `--host` / `--join` on the command line skip the menu.
fn auto_launch(
    mut commands: Commands,
    args: Res<LaunchArgs>,
    prefs: Res<MenuPrefs>,
    mut status: ResMut<MenuStatus>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if args.host {
        start_host(&mut commands, &prefs, &args, &mut status, &mut next_state);
    } else if args.join.is_some() {
        start_join(&mut commands, &prefs, &mut status, &mut next_state);
    }
}

fn on_leave(
    _: On<Activate>,
    mut commands: Commands,
    mut connection: Option<ResMut<Connection>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    net::disconnect(&mut commands, connection.as_deref_mut());
    next_state.set(AppState::Menu);
}

// ---------------------------------------------------------------- connecting

fn spawn_connecting(mut commands: Commands, session: Res<Session>) {
    commands.spawn((
        screen(AppState::Connecting),
        children![(
            panel(),
            children![
                label(format!("Connecting to {}...", session.server_label), 24.0, TEXT),
                (button("Cancel"), observe(on_leave)),
            ],
        )],
    ));
}

// ---------------------------------------------------------------- lobby

#[derive(Component)]
struct LobbyPlayers;

#[derive(Component)]
struct LobbyInfo;

/// Shown only to the lobby leader.
#[derive(Component)]
struct LeaderOnly;

/// Shown only in the team modes.
#[derive(Component)]
struct TeamModeOnly;

/// The text of a button that shows, and changes, the gun in a slot.
#[derive(Component)]
struct LoadoutText(usize);

/// A button that puts the next gun in a slot (0 = primary, 1 = secondary).
fn loadout_button(slot: usize) -> impl Bundle {
    (
        Button,
        Hovered::default(),
        Node {
            padding: UiRect::axes(px(18), px(9)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(px(6)),
            ..default()
        },
        BackgroundColor(BUTTON),
        children![(label("", 20.0, TEXT), Pickable::IGNORE, LoadoutText(slot))],
        observe(move |_: On<Activate>, mut settings: ResMut<Settings>| {
            settings.cycle_gun(slot);
            settings.save();
        }),
    )
}

fn update_loadout_buttons(settings: Res<Settings>, mut texts: Query<(&LoadoutText, &mut Text)>) {
    for (slot, mut text) in &mut texts {
        let (title, gun) = if slot.0 == 0 {
            ("Primary", settings.primary)
        } else {
            ("Secondary", settings.secondary)
        };
        set_text(&mut text, format!("{title}: {}", weapon(gun).name));
    }
}

fn spawn_lobby(mut commands: Commands, session: Res<Session>) {
    let share = match &session.share_address {
        Some(address) => format!(
            "Friends on your network join with:  {address}\nOver the internet: forward this UDP port on your router and share your public IP."
        ),
        None => format!("Connected to {}", session.server_label),
    };
    commands.spawn((
        screen(AppState::Lobby),
        children![(
            panel(),
            children![
                label("LOBBY", 40.0, ACCENT),
                label(share, 16.0, DIM),
                (
                    label("", 20.0, TEXT),
                    LobbyPlayers,
                    TextLayout::no_wrap(),
                    Node {
                        margin: UiRect::vertical(px(10)),
                        ..default()
                    },
                ),
                (label("", 18.0, ACCENT), LobbyInfo),
                (
                    button_row(),
                    children![
                        (button("Start match"), LeaderOnly, observe(send_on_click(ClientMsg::StartMatch))),
                        (button("Change mode"), LeaderOnly, observe(send_on_click(ClientMsg::NextMode))),
                        (button("Change map"), LeaderOnly, observe(send_on_click(ClientMsg::NextMap))),
                        (button("Add bot"), LeaderOnly, observe(send_on_click(ClientMsg::AddBot))),
                        (button("Remove bots"), LeaderOnly, observe(send_on_click(ClientMsg::RemoveBots))),
                    ],
                ),
                (
                    button_row(),
                    children![
                        (button("Switch team"), TeamModeOnly, observe(send_on_click(ClientMsg::SwitchTeam))),
                        loadout_button(0),
                        loadout_button(1),
                        (button("Settings"), observe(on_open_settings)),
                        (button("Leave"), observe(on_leave)),
                    ],
                ),
            ],
        )],
    ));
}

/// An observer that sends `msg` to the server when its button is clicked.
fn send_on_click(msg: ClientMsg) -> impl Fn(On<Activate>, Option<ResMut<Connection>>) {
    move |_, connection| {
        if let Some(mut connection) = connection {
            connection.send(&msg);
        }
    }
}

fn update_lobby(
    session: Res<Session>,
    map: Res<GameMap>,
    mut players: Single<&mut Text, (With<LobbyPlayers>, Without<LobbyInfo>)>,
    mut info: Single<&mut Text, (With<LobbyInfo>, Without<LobbyPlayers>)>,
    mut leader_only: Query<&mut Node, (With<LeaderOnly>, Without<TeamModeOnly>)>,
    mut team_only: Query<&mut Node, (With<TeamModeOnly>, Without<LeaderOnly>)>,
) {
    set_text(&mut players, scoreboard_text(&session));

    let map_name = MAP_NAMES.get(map.0.id as usize).copied().unwrap_or("?");
    let mode_name = MODE_NAMES.get(session.mode as usize).copied().unwrap_or("?");
    let mut lines = vec![format!("{mode_name}   -   map: {map_name}")];
    for mut node in &mut team_only {
        set_shown(&mut node, session.team_mode());
    }
    if let Some(result) = &session.last_result {
        lines.push(result.clone());
    }
    if !session.is_leader() {
        lines.push(format!("Waiting for {} to start the match...", session.name_of(session.leader)));
    }
    set_text(&mut info, lines.join("\n"));

    for mut node in &mut leader_only {
        set_shown(&mut node, session.is_leader());
    }
}

/// Player table, best score first; one table per team in the team modes.
/// Relies on the default font being monospace.
fn scoreboard_text(session: &Session) -> String {
    let mut players: Vec<&PlayerInfo> = session.roster.iter().collect();
    players.sort_by(|a, b| b.kills.cmp(&a.kills).then(a.deaths.cmp(&b.deaths)));
    let row = |player: &PlayerInfo| {
        let marker = if player.id == session.my_id { ">" } else { " " };
        let mut name = player.name.clone();
        if player.id == session.leader {
            name.push_str(" (host)");
        }
        format!("{marker} {:<22}{:>6}{:>8}\n", name, player.kills, player.deaths)
    };
    let heading = |title: &str| format!("  {title:<22}{:>6}{:>8}\n", "KILLS", "DEATHS");

    if !session.team_mode() {
        return heading("PLAYER") + &players.iter().map(|player| row(player)).collect::<String>();
    }
    let mut text = String::new();
    for team in 0..2u8 {
        if team == 1 {
            text.push('\n');
        }
        let title = format!(
            "{} TEAM  -  {}",
            TEAM_NAMES[team as usize].to_uppercase(),
            session.team_scores[team as usize]
        );
        text.push_str(&heading(&title));
        for player in players.iter().filter(|player| player.team & 1 == team) {
            text.push_str(&row(player));
        }
    }
    text
}

// ---------------------------------------------------------------- in-game HUD

/// The pieces of text on the HUD.
#[derive(Component, Clone, Copy)]
enum Hud {
    /// Clock, score and ping along the top.
    Top,
    Health,
    Speed,
    Feed,
    /// Death and respawn message.
    Center,
    Ammo,
    /// The three weapon slots.
    Weapons,
    /// "FRAGGED somebody".
    Kill,
    /// Streaks and revenge, under the kill line.
    Banner,
}

/// The coloured shapes on the HUD.
#[derive(Component, Clone, Copy)]
enum Overlay {
    /// Red flash over the whole screen when hurt.
    DamageFlash,
    /// One of the four ticks that appear around the crosshair on a hit.
    HitTick,
}

#[derive(Component)]
struct ScoreboardPanel;
#[derive(Component)]
struct ScoreboardText;
#[derive(Component)]
struct PausePanel;
#[derive(Component)]
struct PauseInfo;

/// A HUD element pinned to part of the screen. Never blocks mouse clicks.
fn hud_anchor(node: Node) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            ..node
        },
        Pickable::IGNORE,
        DespawnOnExit(AppState::Playing),
    )
}

/// A full-screen HUD layer that centres its content.
fn hud_layer() -> impl Bundle {
    hud_anchor(Node {
        width: percent(100),
        height: percent(100),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        ..default()
    })
}

/// A line across the screen at `top` percent of its height, text centred.
fn hud_line(top: f32) -> impl Bundle {
    hud_anchor(Node {
        width: percent(100),
        top: percent(top),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        row_gap: px(4),
        ..default()
    })
}

/// One tick of the hit marker: a short diagonal stroke `(dx, dy)` away from
/// the middle of its 40 x 40 box.
fn hit_tick(dx: f32, dy: f32) -> impl Bundle {
    const LENGTH: f32 = 12.0;
    const THICKNESS: f32 = 3.0;
    // The strokes point at the crosshair: "\" on one diagonal, "/" on the other.
    let angle = if dx * dy > 0.0 { 45.0 } else { -45.0 };
    (
        Overlay::HitTick,
        Node {
            position_type: PositionType::Absolute,
            left: px(20.0 + dx - LENGTH / 2.0),
            top: px(20.0 + dy - THICKNESS / 2.0),
            width: px(LENGTH),
            height: px(THICKNESS),
            ..default()
        },
        UiTransform::from_rotation(Rot2::degrees(angle)),
        BackgroundColor(Color::NONE),
    )
}

fn spawn_hud(mut commands: Commands) {
    commands.spawn((hud_layer(), Overlay::DamageFlash, BackgroundColor(Color::NONE)));

    // Crosshair.
    commands.spawn((
        hud_layer(),
        children![(
            Node {
                width: px(6),
                height: px(6),
                border_radius: BorderRadius::all(px(3)),
                ..default()
            },
            BackgroundColor(Color::WHITE),
        )],
    ));

    // Hit marker, around the crosshair.
    commands.spawn((
        hud_layer(),
        children![(
            Node {
                width: px(40),
                height: px(40),
                ..default()
            },
            children![
                hit_tick(-11.0, -11.0),
                hit_tick(11.0, -11.0),
                hit_tick(-11.0, 11.0),
                hit_tick(11.0, 11.0),
            ],
        )],
    ));

    // Death / respawn message, a bit above the middle of the screen.
    commands.spawn((hud_line(30.0), children![(label("", 30.0, DANGER), Hud::Center)]));

    // Kill announcements, a bit below it.
    commands.spawn((
        hud_line(60.0),
        children![
            (label("", 28.0, TEXT), Hud::Kill),
            (label("", 22.0, ACCENT), Hud::Banner),
        ],
    ));

    commands.spawn((
        hud_anchor(Node {
            width: percent(100),
            top: px(12),
            justify_content: JustifyContent::Center,
            ..default()
        }),
        children![(label("", 22.0, TEXT), Hud::Top)],
    ));

    commands.spawn((
        hud_anchor(Node {
            left: px(24),
            bottom: px(18),
            ..default()
        }),
        children![(label("", 44.0, TEXT), Hud::Health)],
    ));

    commands.spawn((
        hud_anchor(Node {
            width: percent(100),
            bottom: px(22),
            justify_content: JustifyContent::Center,
            ..default()
        }),
        children![(label("", 20.0, TEXT), Hud::Speed)],
    ));

    commands.spawn((
        hud_anchor(Node {
            right: px(18),
            top: px(52),
            ..default()
        }),
        children![(label("", 18.0, TEXT), Hud::Feed, TextLayout::justify(Justify::Right))],
    ));

    commands.spawn((
        hud_anchor(Node {
            right: px(24),
            bottom: px(18),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::End,
            row_gap: px(4),
            ..default()
        }),
        children![
            (label("", 16.0, DIM), Hud::Weapons),
            (label("", 32.0, TEXT), Hud::Ammo),
        ],
    ));

    commands.spawn((
        hud_layer(),
        ScoreboardPanel,
        children![(
            panel(),
            children![(label("", 20.0, TEXT), ScoreboardText, TextLayout::no_wrap())]
        )],
    ));

    // Pause menu. Spawned last so it is on top; this one does take clicks.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            display: Display::None,
            ..default()
        },
        PausePanel,
        GlobalZIndex(10),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
        DespawnOnExit(AppState::Playing),
        children![(
            panel(),
            children![
                label("PAUSED", 36.0, ACCENT),
                (label("", 17.0, DIM), PauseInfo),
                (button_row(), children![loadout_button(0), loadout_button(1)]),
                label("A new loadout is handed out the next time you spawn.", 15.0, DIM),
                (
                    button_row(),
                    children![
                        (button("Resume"), observe(on_resume)),
                        (button("Settings"), observe(on_open_settings)),
                        (button("End match"), LeaderOnly, observe(send_on_click(ClientMsg::EndMatch))),
                        (button("Leave"), observe(on_leave)),
                    ],
                ),
            ],
        )],
    ));
}

fn on_resume(_: On<Activate>, mut controls: ResMut<Controls>) {
    controls.captured = true;
}

fn format_clock(seconds: f32) -> String {
    let total = seconds.max(0.0).ceil() as u32;
    format!("{}:{:02}", total / 60, total % 60)
}

/// The line along the top: clock, score and ping.
fn top_line(session: &Session, ping: f64) -> String {
    let clock = format_clock(session.time_left);
    if !session.team_mode() {
        let my_kills = session
            .roster
            .iter()
            .find(|p| p.id == session.my_id)
            .map_or(0, |p| p.kills);
        let lead = match session.roster.iter().max_by_key(|p| p.kills) {
            Some(p) if p.kills > 0 => format!("{} leads with {}", p.name, p.kills),
            _ => "no kills yet".to_string(),
        };
        return format!(
            "{clock}   |   you {my_kills} / {}   |   {lead}   |   {ping:.0} ms",
            session.score_limit
        );
    }
    let [red, blue] = session.team_scores;
    let mut line = format!(
        "{clock}   |   RED {red}  :  {blue} BLUE   |   first to {}   |   you are {}",
        session.score_limit,
        TEAM_NAMES[session.team_of(session.my_id) as usize].to_uppercase()
    );
    if session.mode == MODE_HILL {
        line.push_str(match session.hill {
            HILL_EMPTY => "   |   hill: empty",
            HILL_CONTESTED => "   |   hill: CONTESTED",
            held if held == HILL_HELD => "   |   hill: RED",
            _ => "   |   hill: BLUE",
        });
    }
    line.push_str(&format!("   |   {ping:.0} ms"));
    line
}

fn update_hud(
    session: Res<Session>,
    settings: Res<Settings>,
    player: Res<LocalPlayer>,
    feed: Res<KillFeed>,
    notices: Res<Notices>,
    connection: Option<Res<Connection>>,
    mut texts: Query<(&Hud, &mut Text, &mut TextColor)>,
) {
    let gun = player.guns.get(player.slot);
    let stats = player.weapon();
    // A notice fades out over its last half second.
    let notice = |notice: &Option<(String, f32)>, color: Color| match notice {
        Some((text, left)) => (text.clone(), color.with_alpha((left / 0.5).min(1.0))),
        None => (String::new(), color),
    };
    for (hud, mut text, mut color) in &mut texts {
        let (value, wanted) = match hud {
            Hud::Top => (
                top_line(&session, connection.as_ref().map_or(0.0, |c| c.ping_ms())),
                TEXT,
            ),
            Hud::Health => (
                format!("+ {}", player.health.max(0)),
                if player.health <= 30 { DANGER } else { TEXT },
            ),
            Hud::Speed => {
                let dash = if player.dash_cooldown <= 0.0 {
                    "DASH ready".to_string()
                } else {
                    let filled = (10.0 * (1.0 - player.dash_cooldown / DASH_COOLDOWN)) as usize;
                    format!("DASH [{}{}]", "#".repeat(filled), "-".repeat(10 - filled.min(10)))
                };
                (format!("{:>5.1} m/s    {}", player.horizontal_speed(), dash), TEXT)
            }
            Hud::Feed => (
                feed.0.iter().map(|(line, _)| line.as_str()).collect::<Vec<_>>().join("\n"),
                TEXT,
            ),
            Hud::Center => {
                let message = match (&player.killed_by, player.alive) {
                    (_, true) => String::new(),
                    (Some(killer), false) => format!("Fragged by {killer}  -  respawning..."),
                    (None, false) => "Get ready...".to_string(),
                };
                (message, DANGER)
            }
            Hud::Ammo => match gun {
                Some(_) if player.reloading() => ("RELOADING".to_string(), ACCENT),
                Some(gun) => (
                    format!("{} / {}", gun.ammo, stats.magazine),
                    // Running low.
                    if gun.ammo * 4 <= stats.magazine { ACCENT } else { TEXT },
                ),
                None => (String::new(), TEXT),
            },
            Hud::Weapons => {
                let slots = [Action::Primary, Action::Secondary, Action::Knife];
                let names = [
                    weapon(player.guns[0].weapon).name,
                    weapon(player.guns[1].weapon).name,
                    weapon(KNIFE).name,
                ];
                let mut line = String::new();
                for (slot, (action, name)) in slots.into_iter().zip(names).enumerate() {
                    let key = settings.bind(action).display();
                    line.push_str(&if slot == player.slot {
                        format!("{key} [{}]   ", name.to_uppercase())
                    } else {
                        format!("{key} {name}   ")
                    });
                }
                line.push_str(&format!(
                    "|   {} grenades {}",
                    settings.bind(Action::Grenade).display(),
                    player.grenades.floor()
                ));
                (line, DIM)
            }
            Hud::Kill => notice(&notices.kill, TEXT),
            Hud::Banner => notice(&notices.banner, ACCENT),
        };
        set_text(&mut text, value);
        if color.0 != wanted {
            color.0 = wanted;
        }
    }
}

fn update_overlays(
    input: ActionInput,
    settings: Res<Settings>,
    session: Res<Session>,
    player: Res<LocalPlayer>,
    controls: Res<Controls>,
    mut overlays: Query<(&Overlay, &mut BackgroundColor)>,
    mut nodes: ParamSet<(
        Single<&mut Node, With<ScoreboardPanel>>,
        Single<&mut Node, With<PausePanel>>,
        Query<&mut Node, With<LeaderOnly>>,
    )>,
    mut texts: ParamSet<(
        Single<&mut Text, With<ScoreboardText>>,
        Single<&mut Text, With<PauseInfo>>,
    )>,
) {
    // White ticks for a hit; red, and held a little longer, for a kill.
    let tick_color = if player.kill_marker > 0.0 {
        Color::srgba(1.0, 0.15, 0.1, (player.kill_marker * 3.0).min(1.0))
    } else {
        Color::srgba(1.0, 1.0, 1.0, (player.hit_marker / HIT_MARKER_TIME * 2.0).clamp(0.0, 1.0))
    };
    for (overlay, mut background) in &mut overlays {
        let wanted = match overlay {
            Overlay::DamageFlash => Color::srgba(0.8, 0.0, 0.0, player.damage_flash * 0.35),
            Overlay::HitTick => tick_color,
        };
        if background.0 != wanted {
            background.0 = wanted;
        }
    }

    let paused = !controls.captured;
    let show_scores = input.pressed(Action::Scores);
    set_shown(&mut nodes.p0(), show_scores);
    set_shown(&mut nodes.p1(), paused);
    for mut node in &mut nodes.p2() {
        set_shown(&mut node, session.is_leader());
    }
    if show_scores {
        set_text(&mut texts.p0(), scoreboard_text(&session));
    }
    if paused {
        let key = |action| settings.bind(action).display();
        set_text(
            &mut texts.p1(),
            format!(
                "{} {} {} {} move   {} jump (hold to bunny hop, tap in the air to double jump)\n\
                 {} dash   {} crouch (while running: slide)   {} scores\n\
                 {} fire   {} aim   {} reload   {} grenade   {} {} {} weapons\n\
                 Strafe and turn in the air to build speed. Green pads launch you.\n\
                 Bullets take time to arrive: lead moving targets. Health returns after {} s unhurt.\n\
                 Keys, mouse and volume are under Settings.",
                key(Action::Forward),
                key(Action::Left),
                key(Action::Back),
                key(Action::Right),
                key(Action::Jump),
                key(Action::Dash),
                key(Action::Crouch),
                key(Action::Scores),
                key(Action::Fire),
                key(Action::Aim),
                key(Action::Reload),
                key(Action::Grenade),
                key(Action::Primary),
                key(Action::Secondary),
                key(Action::Knife),
                REGEN_DELAY,
            ),
        );
    }
}
