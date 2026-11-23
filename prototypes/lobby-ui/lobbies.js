const MAX_PLAYERS = 16;
const MODE_NAMES = ["Free for all", "Team deathmatch", "King of the hill", "Conquest"];
const MAP_NAMES = ["Arena", "Depot", "Valley"];

const LOBBIES = [
  { id: 0, host: "nova", name: "Friday frags", mode: 0, map: 0, players: 11, ping: 38, locked: false, address: "203.0.113.21:7777", where: "online", state: "match", timeLeft: 252, bots: 0 },
  { id: 1, host: "blitz", name: "Valley conquest", mode: 3, map: 2, players: 14, ping: 72, locked: false, address: "198.51.100.40:7777", where: "online", state: "match", timeLeft: 610, bots: 0 },
  { id: 2, host: "ember", name: "Depot duels", mode: 0, map: 1, players: 2, ping: 55, locked: false, address: "203.0.113.88:7777", where: "online", state: "lobby", timeLeft: 0, bots: 0 },
  { id: 3, host: "quartz", name: "Scrim night", mode: 1, map: 0, players: 8, ping: 91, locked: true, address: "198.51.100.12:7777", where: "online", state: "lobby", timeLeft: 0, bots: 0 },
  { id: 4, host: "pixel", name: "Bots practice", mode: 2, map: 1, players: 1, ping: 140, locked: false, address: "203.0.113.150:7777", where: "online", state: "match", timeLeft: 95, bots: 4 },
  { id: 5, host: "moss", name: "Hill rush", mode: 2, map: 2, players: 16, ping: 64, locked: false, address: "198.51.100.77:7777", where: "online", state: "match", timeLeft: 31, bots: 0 },
  { id: 6, host: "kai", name: "House LAN", mode: 0, map: 0, players: 3, ping: 2, locked: false, address: "192.168.1.34:7777", where: "lan", state: "lobby", timeLeft: 0, bots: 2 },
];

const SAVED = [
  { address: "203.0.113.21:7777", label: "nova's lobby" },
  { address: "198.51.100.12:7777", label: "Scrim night" },
  { address: "198.51.100.200:7777", label: "Old weekend lobby" },
  { address: "192.168.1.34:7777", label: "House LAN" },
];

const LOBBY_TABS = [
  { id: "online", label: "Online" },
  { id: "lan", label: "LAN" },
  { id: "recent", label: "Recent" },
  { id: "favorites", label: "Favorites" },
];

const lobbyUi = {
  type: "mix",
  size: "1280x720",
  tab: "online",
  hideFull: false,
  hideLocked: false,
  sort: "ping",
  status: "",
  statusKind: "",
  detail: null,
  detailError: "",
  browse: false,
  favorites: new Set(["203.0.113.21:7777", "192.168.1.34:7777"]),
  updated: performance.now(),
};

function lobbyByAddress(address) {
  return LOBBIES.find((lobby) => lobby.address === address) || null;
}

function isFull(lobby) {
  return lobby.players >= MAX_PLAYERS;
}

function pingLevel(ms) {
  if (ms < 50) return 4;
  if (ms < 100) return 3;
  if (ms < 150) return 2;
  return 1;
}

function pingHtml(ms) {
  if (ms === null) return `<span class="lb-ping is-off" title="Not reachable">offline</span>`;
  const level = pingLevel(ms);
  const bars = [1, 2, 3, 4].map((n) => `<i class="${n <= level ? "is-on" : ""}"></i>`).join("");
  return `<span class="lb-ping lv-${level}" title="${ms} ms"><span class="lb-bars">${bars}</span>${ms} ms</span>`;
}

function lockHtml(lobby) {
  return lobby.locked ? `<span class="lb-lock" title="Needs a password">&#128273;</span>` : "";
}

function playersHtml(lobby) {
  const part = (lobby.players / MAX_PLAYERS) * 100;
  return `<span class="lb-players${isFull(lobby) ? " is-full" : ""}"><b>${lobby.players}/${MAX_PLAYERS}</b><span class="lb-fill"><i style="width:${part.toFixed(0)}%"></i></span></span>`;
}

function starHtml(address) {
  const on = lobbyUi.favorites.has(address);
  return `<button class="lb-star${on ? " is-on" : ""}" data-act="star" data-address="${address}" title="${on ? "Remove from favorites" : "Add to favorites"}">${on ? "&#9733;" : "&#9734;"}</button>`;
}

function lobbyRow(lobby) {
  const full = isFull(lobby) ? " is-full" : "";
  return `<div class="lb-row${full}">
    ${starHtml(lobby.address)}
    <button class="lb-main" data-act="join" data-id="${lobby.id}">
      <span class="lb-name">${lockHtml(lobby)}${lobby.name}<small>${lobby.host} · ${lobby.address}</small></span>
      <span class="lb-tag">${MODE_NAMES[lobby.mode]}</span>
      <span class="lb-tag">${MAP_NAMES[lobby.map]}</span>
      ${playersHtml(lobby)}
      ${pingHtml(lobby.ping)}
    </button>
  </div>`;
}

function offlineRow(saved) {
  return `<div class="lb-row is-offline">
    ${starHtml(saved.address)}
    <span class="lb-main">
      <span class="lb-name">${saved.label}<small>${saved.address}</small></span>
      <span class="lb-tag">Not running</span>
      ${pingHtml(null)}
    </span>
  </div>`;
}

function savedEntries(filter) {
  return SAVED.filter(filter).map((saved) => lobbyByAddress(saved.address) || saved);
}

function tabEntries(tab) {
  if (tab === "lan") return LOBBIES.filter((lobby) => lobby.where === "lan");
  if (tab === "recent") return savedEntries(() => true);
  if (tab === "favorites") return savedEntries((saved) => lobbyUi.favorites.has(saved.address));
  return LOBBIES.filter((lobby) => lobby.where === "online");
}

function keepEntry(entry) {
  if (entry.id === undefined) return true;
  if (lobbyUi.hideFull && isFull(entry)) return false;
  return !(lobbyUi.hideLocked && entry.locked);
}

function sortValue(entry) {
  if (entry.id === undefined) return Infinity;
  return lobbyUi.sort === "ping" ? entry.ping : -entry.players;
}

function entryRow(entry) {
  return entry.id === undefined ? offlineRow(entry) : lobbyRow(entry);
}

function listHtml(entries) {
  const rows = entries.filter(keepEntry).sort((a, b) => sortValue(a) - sortValue(b)).map(entryRow);
  return rows.length ? rows.join("") : `<div class="lb-empty">No lobbies here right now.</div>`;
}

function fieldHtml(caption, value, placeholder, cls) {
  const head = caption ? `<span class="section">${caption}</span>` : "";
  return `<label class="mm-field ${cls || ""}">${head}<input value="${value}" placeholder="${placeholder}" spellcheck="false"></label>`;
}

function identityCard() {
  return `<div class="card mm-row-card">${fieldHtml("Your name", "Player", "")}${fieldHtml("Lobby password (optional)", "", "")}</div>`;
}

function hostCard(withListToggle) {
  const toggle = withListToggle
    ? `<label class="mm-check"><input type="checkbox" checked> Show in the online list</label>`
    : "";
  return `<div class="card mm-card"><div class="mm-card-title">Host</div><div class="dim mm-hint">Start a lobby on this PC.</div>
    ${fieldHtml("UDP port", "7777", "")}${toggle}<button class="btn btn-primary" data-act="host">Host lobby</button></div>`;
}

function addressRow(compact) {
  return `<div class="mm-address${compact ? " is-compact" : ""}">${fieldHtml(compact ? "" : "Address", "", compact ? "Or type an address, like 203.0.113.7" : "203.0.113.7", "mm-grow")}<button class="btn btn-small" data-act="paste">Paste</button>${compact ? `<button class="btn btn-primary btn-small" data-act="direct">Join</button>` : ""}</div>`;
}

function joinCard() {
  return `<div class="card mm-card"><div class="mm-card-title">Join</div><div class="dim mm-hint">Connect to a friend's lobby.</div>
    ${addressRow(false)}<button class="btn btn-primary" data-act="direct">Join lobby</button></div>`;
}

function refreshButton() {
  return `<button class="lb-refresh" data-act="refresh" title="Refresh">&#8635;</button>`;
}

function tabsHtml(withRefresh = true) {
  return `<div class="lb-tabs">${LOBBY_TABS.map((tab) => {
    const on = lobbyUi.tab === tab.id ? " is-on" : "";
    return `<button class="lb-tab${on}" data-act="tab" data-tab="${tab.id}">${tab.label}<small>${tabEntries(tab.id).length}</small></button>`;
  }).join("")}${withRefresh ? refreshButton() : ""}</div>`;
}

function filtersHtml() {
  const check = (key, text) => `<label class="mm-check"><input type="checkbox" data-act="filter" data-key="${key}"${lobbyUi[key] ? " checked" : ""}> ${text}</label>`;
  const sort = (key, text) => `<button class="lb-sort${lobbyUi.sort === key ? " is-on" : ""}" data-act="sort" data-key="${key}">${text}</button>`;
  return `<div class="lb-filters">${check("hideFull", "Hide full")}${check("hideLocked", "Hide locked")}<span class="lb-sorts">Sort ${sort("ping", "Ping")}${sort("players", "Players")}</span></div>`;
}

function browserCard() {
  return `<div class="card mm-card mm-browser"><div class="mm-card-title">Join</div>${tabsHtml()}${filtersHtml()}
    <div class="lb-list">${listHtml(tabEntries(lobbyUi.tab))}</div>
    ${addressRow(true)}</div>`;
}

function savedCard() {
  return `<div class="card mm-card mm-browser"><div class="mm-card-title">Your lobbies <button class="lb-refresh" data-act="refresh" title="Refresh">&#8635;</button></div>
    <div class="dim mm-hint">Lobbies you joined or starred. Each one is asked for its status, so no central server is needed.</div>
    <div class="lb-list">${listHtml(savedEntries(() => true))}</div>
    ${addressRow(true)}</div>`;
}

function bestLobby() {
  const open = LOBBIES.filter((lobby) => !lobby.locked && !isFull(lobby) && lobby.where === "online");
  return open.sort((a, b) => b.players - a.players || a.ping - b.ping)[0] || null;
}

function quickBar() {
  const best = bestLobby();
  const hint = best ? `Joins the fullest open lobby with a good ping. Now: ${best.name}, ${best.players}/${MAX_PLAYERS}, ${best.ping} ms.` : "No open lobby right now.";
  const online = tabEntries("online").length + tabEntries("lan").length;
  const line = lobbyUi.status ? `<span class="mm-status ${lobbyUi.statusKind}">${lobbyUi.status}</span>` : `<span class="dim">${hint}</span>`;
  return `<div class="mm-quick"><button class="btn btn-primary btn-big" data-act="quick">Quick play</button>
    <div class="mm-quick-side">${line}<button class="btn btn-small" data-act="browse">Browse lobbies (${online})</button></div></div>`;
}

function statusHtml() {
  return lobbyUi.status ? `<div class="mm-status ${lobbyUi.statusKind}">${lobbyUi.status}</div>` : "";
}

function bottomHtml() {
  return `<div class="buttons"><button class="btn">Paint yourself</button><button class="btn">Settings</button><button class="btn btn-danger">Quit</button></div>`;
}

function headHtml() {
  return `<h1 class="title mm-title">SHITSHOOTER</h1><div class="dim mm-sub">fast arena shooter, hosted by you</div>`;
}

const MENU_TYPES = {
  now: () => `${identityCard()}<div class="mm-cards">${hostCard(false)}${joinCard()}</div>`,
  list: () => `${identityCard()}<div class="mm-cards">${hostCard(true)}${browserCard()}</div>`,
  quick: () => `${identityCard()}${quickBar()}<div class="mm-cards">${hostCard(true)}${joinCard()}</div>`,
  saved: () => `${identityCard()}<div class="mm-cards">${hostCard(false)}${savedCard()}</div>`,
  live: () => `${identityCard()}<div class="mm-cards">${hostCard(true)}${joinCard()}</div>`,
  mix: () => `${identityCard()}<div class="mm-cards">${hostCard(true)}${joinCard()}</div>`,
};

function agoHtml() {
  const seconds = Math.round((performance.now() - lobbyUi.updated) / 1000);
  return `<span class="dim lv-ago">updated ${seconds} s ago</span>`;
}

function justChanged(lobby) {
  return Boolean(lobby.changedAt) && performance.now() - lobby.changedAt < 2000;
}

function mixCard(lobby) {
  const classes = ["lv-card", isFull(lobby) ? "is-full" : "", justChanged(lobby) ? "is-changed" : ""].filter(Boolean).join(" ");
  return `<div class="${classes}">${starHtml(lobby.address)}<button class="lv-main" data-act="join" data-id="${lobby.id}">
    <span class="lv-top"><b>${lockHtml(lobby)}${lobby.name}</b>${pingHtml(lobby.ping)}</span>
    <span class="lv-meta">${MODE_NAMES[lobby.mode]} · ${MAP_NAMES[lobby.map]} · ${lobby.host}</span>
    ${playersHtml(lobby)}
  </button></div>`;
}

function mixOffline(saved) {
  return `<div class="lv-card is-offline">${starHtml(saved.address)}<span class="lv-main">
    <span class="lv-top"><b>${saved.label}</b>${pingHtml(null)}</span>
    <span class="lv-meta">${saved.address} · Not running</span>
  </span></div>`;
}

function mixListHtml(entries) {
  const cards = entries.filter(keepEntry).sort((a, b) => sortValue(a) - sortValue(b));
  const html = cards.map((entry) => (entry.id === undefined ? mixOffline(entry) : mixCard(entry)));
  return html.length ? html.join("") : `<div class="lb-empty">No lobbies here right now.</div>`;
}

function mixHtml() {
  const best = bestLobby();
  const hint = best ? `Joins ${best.name} · ${best.players}/${MAX_PLAYERS} · ${best.ping} ms` : "No open lobby right now.";
  return `<div class="mx-panel">
    <div class="lv-head"><span class="section">Live now</span>${agoHtml()}${refreshButton()}</div>
    <button class="btn btn-primary mx-quick" data-act="quick">Quick play</button>
    <div class="dim mx-hint">${hint}</div>
    ${tabsHtml(false)}${filtersHtml()}
    <div class="mx-list">${mixListHtml(tabEntries(lobbyUi.tab))}</div>
  </div>`;
}

const SIDE_TYPES = { live: () => liveHtml(), mix: () => mixHtml() };

function sideHtml() {
  const side = SIDE_TYPES[lobbyUi.type];
  return side ? side() : "";
}

function renderSide() {
  document.getElementById("side-root").innerHTML = sideHtml();
}

function updateAgo() {
  const ago = document.querySelector("#side-root .lv-ago");
  if (ago) ago.outerHTML = agoHtml();
}

function liveCard(lobby) {
  return `<button class="lv-card${isFull(lobby) ? " is-full" : ""}" data-act="join" data-id="${lobby.id}">
    <span class="lv-top"><b>${lockHtml(lobby)}${lobby.name}</b>${pingHtml(lobby.ping)}</span>
    <span class="lv-meta">${MODE_NAMES[lobby.mode]} · ${MAP_NAMES[lobby.map]} · ${lobby.host}</span>
    ${playersHtml(lobby)}
  </button>`;
}

function liveHtml() {
  const lobbies = LOBBIES.slice().sort((a, b) => isFull(a) - isFull(b) || b.players - a.players).slice(0, 5);
  return `<div class="lv-panel"><div class="lv-head"><span class="section">Live now</span>${agoHtml()}</div>
    ${lobbies.map(liveCard).join("")}<button class="btn btn-small" data-act="browse">See all ${LOBBIES.length} lobbies</button></div>`;
}

const HANDLES = ["rook", "vex", "jinx", "lumen", "sable", "drift", "onyx", "rift", "zest", "halo", "fern", "mako", "echo", "juno", "pike", "wren", "flint", "orbit"];
const PLAYER_COLORS = ["#ff4a44", "#4a90ff", "#4adb5c", "#ffb13b", "#c77dff", "#3fd0d4", "#ff7ab8", "#e8e8e8"];
const TEAM_MODES = [1, 3];

function playerColor(lobby, index) {
  if (TEAM_MODES.includes(lobby.mode)) return index % 2 ? "#6aa8ff" : "#ff6b61";
  return PLAYER_COLORS[index % PLAYER_COLORS.length];
}

function rosterEntry(lobby, index) {
  const human = index < lobby.players;
  const name = index === 0 ? lobby.host : HANDLES[(index + lobby.id * 3) % HANDLES.length];
  const playing = lobby.state === "match";
  return {
    name: human ? name : `Bot ${index - lobby.players + 1}`,
    host: index === 0,
    color: playerColor(lobby, index),
    kills: playing ? (index * 7 + lobby.id * 5 + 3) % 23 : 0,
    deaths: playing ? (index * 5 + lobby.id * 3 + 1) % 17 : 0,
  };
}

function roster(lobby) {
  const entries = Array.from({ length: lobby.players + lobby.bots }, (_, index) => rosterEntry(lobby, index));
  return lobby.state === "match" ? entries.sort((a, b) => b.kills - a.kills) : entries;
}

function rosterRow(player) {
  const tag = player.host ? "<em>HOST</em>" : "";
  return `<div class="roster-row"><i style="background:${player.color}"></i><span>${player.name}${tag}</span><b>${player.kills}</b><b class="dim">${player.deaths}</b></div>`;
}

function clockText(seconds) {
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

function stateText(lobby) {
  return lobby.state === "match" ? `In match · ${clockText(lobby.timeLeft)} left` : "Waiting in the lobby";
}

function matchRow(label, value) {
  return `<div class="match-row"><span>${label}</span>${value}</div>`;
}

function detailJoinHtml(lobby) {
  if (isFull(lobby)) return `<button class="btn" disabled>Full · ${MAX_PLAYERS}/${MAX_PLAYERS}</button>`;
  return `<button class="btn btn-primary" data-act="detail-join">Join lobby</button>`;
}

function detailExtras(lobby) {
  const password = lobby.locked ? `<div class="dt-password">${fieldHtml("Password", "", "This lobby needs a password")}</div>` : "";
  const error = lobbyUi.detailError ? `<div class="mm-status is-bad">${lobbyUi.detailError}</div>` : "";
  return password + error;
}

function matchCard(lobby) {
  return `<div class="card dt-match"><div class="section">Match</div>
    ${matchRow("Mode", MODE_NAMES[lobby.mode])}${matchRow("Map", MAP_NAMES[lobby.map])}${matchRow("Bots", lobby.bots)}
    ${matchRow("State", stateText(lobby))}${matchRow("Ping", pingHtml(lobby.ping))}${matchRow("Password", lobby.locked ? "Yes" : "No")}</div>`;
}

function playersCard(lobby) {
  return `<div class="card dt-players"><div class="section">Players ${lobby.players}/${MAX_PLAYERS}</div>
    <div class="roster-head"><i></i><span>PLAYER</span><span>KILLS</span><span>DEATHS</span></div>
    <div class="dt-roster">${roster(lobby).map(rosterRow).join("")}</div></div>`;
}

function detailHtml() {
  const lobby = LOBBIES[lobbyUi.detail];
  return `<div class="mm-modal"><div class="panel dt-panel">
    <h1 class="title dt-title">${lockHtml(lobby)}${lobby.name}</h1>
    <div class="dim dt-sub">Hosted by ${lobby.host} · ${lobby.address}</div>
    <div class="dt-cards">${playersCard(lobby)}${matchCard(lobby)}</div>
    ${detailExtras(lobby)}
    <div class="buttons">${detailJoinHtml(lobby)}<button class="btn" data-act="close">Back</button></div>
  </div></div>`;
}

function browseHtml() {
  return `<div class="mm-modal"><div class="panel mm-dialog mm-browse"><div class="mm-browse-head"><span class="mm-card-title">Lobbies</span><button class="btn btn-small" data-act="close">Close</button></div>
    ${tabsHtml()}${filtersHtml()}<div class="lb-list">${listHtml(tabEntries(lobbyUi.tab))}</div></div></div>`;
}

function modalHtml() {
  if (lobbyUi.detail !== null) return detailHtml();
  return lobbyUi.browse ? browseHtml() : "";
}

function renderLobbies() {
  const menu = document.getElementById("menu-panel");
  menu.className = `panel mm-panel mm-type-${lobbyUi.type}`;
  const status = lobbyUi.type === "quick" ? "" : statusHtml();
  menu.innerHTML = `${headHtml()}${MENU_TYPES[lobbyUi.type]()}${status}${bottomHtml()}`;
  renderSide();
  document.getElementById("modal-root").innerHTML = modalHtml();
}

function setStatus(text, kind) {
  lobbyUi.status = text;
  lobbyUi.statusKind = kind;
}

function joinLobby(lobby) {
  lobbyUi.browse = false;
  lobbyUi.detail = null;
  lobbyUi.detailError = "";
  const how = lobby.locked ? " with the password" : "";
  setStatus(`Joining ${lobby.name} at ${lobby.address}${how} (demo).`, "is-good");
}

function openDetail(id) {
  lobbyUi.detail = id;
  lobbyUi.detailError = "";
}

function refreshLobbies() {
  LOBBIES.forEach((lobby) => {
    lobby.ping = Math.max(1, lobby.ping + Math.round(Math.random() * 16 - 8));
    const players = Math.min(MAX_PLAYERS, Math.max(1, lobby.players + Math.round(Math.random() * 2 - 1)));
    if (players !== lobby.players) lobby.changedAt = performance.now();
    lobby.players = players;
  });
  lobbyUi.updated = performance.now();
}

const LOBBY_ACTIONS = {
  join: (data) => openDetail(Number(data.id)),
  tab: (data) => {
    lobbyUi.tab = data.tab;
  },
  sort: (data) => {
    lobbyUi.sort = data.key;
  },
  filter: (data) => {
    lobbyUi[data.key] = !lobbyUi[data.key];
  },
  star: (data) => {
    const set = lobbyUi.favorites;
    if (set.has(data.address)) set.delete(data.address);
    else set.add(data.address);
  },
  refresh: () => {
    refreshLobbies();
    setStatus("Lobby list updated.", "");
  },
  quick: () => {
    const best = bestLobby();
    return best ? joinLobby(best) : setStatus("No open lobby right now. Host one instead.", "is-bad");
  },
  browse: () => {
    lobbyUi.browse = true;
  },
  close: () => {
    if (lobbyUi.detail === null) lobbyUi.browse = false;
    lobbyUi.detail = null;
    lobbyUi.detailError = "";
  },
  "detail-join": () => {
    const lobby = LOBBIES[lobbyUi.detail];
    const field = document.querySelector(".dt-password input");
    if (lobby.locked && !(field && field.value)) {
      lobbyUi.detailError = "Type the lobby password first.";
      return;
    }
    joinLobby(lobby);
  },
  host: () => setStatus("Hosting on UDP port 7777 (demo).", "is-good"),
  direct: () => setStatus("Joining the address (demo).", "is-good"),
  paste: () => setStatus("Pasted the address from the clipboard (demo).", ""),
};

function onLobbyClick(event) {
  const el = event.target.closest("[data-act]");
  if (!el) return;
  LOBBY_ACTIONS[el.dataset.act](el.dataset);
  renderLobbies();
}
