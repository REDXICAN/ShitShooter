const HUD_KEYS = ["1", "2", "3"];
const HUD_TICK = 0.05;
const DRAW_TIME = 0.35;
const IDLE_AFTER = 3;

function hudItem(state, slot) {
  if (slot === 2) return { kind: "knife", id: 0, sight: 0, name: KNIFE.name, sightName: "", magazine: 0, reload: 0, interval: 0.2, auto: true };
  const id = state[SLOT_KEYS[slot]];
  const gun = GUNS[id];
  const sight = sightFor(state, id);
  const sightName = gun.sights.length ? SIGHTS[sight].name : "";
  return { kind: "gun", id, sight, name: gun.name, sightName, magazine: gun.magazine, reload: gun.reload, interval: gun.interval, auto: gun.auto };
}

function hudImg(item, cls) {
  return imgHtml(item.kind, item.id, item.sight, cls, item.name);
}

function hudKey(slot, on) {
  return `<span class="hud-key${on ? " is-on" : ""}">${HUD_KEYS[slot]}</span>`;
}

function isLow(v) {
  return v.ammo !== null && v.ammo * 4 <= v.item.magazine;
}

function hudGrenadeChip(state, count) {
  const grenade = GRENADES[state.grenade];
  const pips = Array.from({ length: grenade.max }, (_, i) => `<i class="${count >= i + 1 ? "" : "is-dim"}"></i>`).join("");
  return `<div class="hud-row hud-end"><span class="hud-key">${BINDS.grenade}</span><div class="hud-chip"><span class="hud-gname">${grenade.name.toUpperCase()}</span><span class="hud-pips">${pips}</span></div></div>`;
}

function grenadeIcon(state, count, index) {
  const fill = Math.max(0, Math.min(1, count - index));
  const ready = fill >= 1 ? " is-ready" : "";
  const empty = `${((1 - fill) * 100).toFixed(1)}%`;
  return `<span class="hud-gicon${ready}" style="--empty:${empty}">${grenadeImg(state.grenade, "hud-gicon-base")}${grenadeImg(state.grenade, "hud-gicon-fill")}</span>`;
}

function grenadeIcons(state, count) {
  const grenade = GRENADES[state.grenade];
  const icons = Array.from({ length: grenade.max }, (_, index) => grenadeIcon(state, count, index)).join("");
  return `<div class="hud-gicons" title="${grenade.name} grenade"><span class="hud-glabel">GRENADE</span>${icons}</div>`;
}

function grenadeCount(state, count) {
  const ready = Math.floor(count);
  return `<div class="hud-gcount${ready ? "" : " is-dim"}" title="${GRENADES[state.grenade].name} grenade"><span class="hud-glabel">GRENADE</span><b>${ready}</b>${grenadeImg(state.grenade, "hud-gcount-img")}</div>`;
}

function hudTitle(v) {
  const text = v.reloading ? "RELOADING" : v.item.name.toUpperCase();
  return `<span class="hud-title${v.reloading ? " is-accent" : ""}">${text}</span>`;
}

function ammoClass(v) {
  if (isLow(v)) return "is-accent";
  return v.ammo === v.item.magazine ? "is-full" : "";
}

function hudAmmo(v) {
  if (v.ammo === null) return "";
  return `<span class="hud-ammo"><b class="${ammoClass(v)}">${v.ammo}</b><i>/ ${v.item.magazine}</i></span>`;
}

function dashHtml(v) {
  return `<span class="hud-ammo"><b class="is-accent">--</b><i>/ ${v.item.magazine}</i></span>`;
}

function ringHtml(v) {
  const radius = 13;
  const length = 2 * Math.PI * radius;
  const fill = `${(length * v.part).toFixed(1)} ${length.toFixed(1)}`;
  return `<span class="hud-ring"><svg viewBox="0 0 32 32"><circle class="hud-ring-track" cx="16" cy="16" r="${radius}"></circle><circle class="hud-ring-fill" cx="16" cy="16" r="${radius}" stroke-dasharray="${fill}"></circle></svg><b>${v.reloadLeft.toFixed(1)}</b></span>`;
}

function countHtml(v) {
  const magazine = v.item.magazine;
  return `<span class="hud-ammo"><b class="is-accent">${Math.floor(v.part * magazine)}</b><i>/ ${magazine}</i></span>`;
}

function refillHtml(v) {
  const filled = Math.floor(v.part * v.item.magazine);
  const marks = Array.from({ length: v.item.magazine }, (_, i) => `<i class="${i < filled ? "" : "is-spent"}"></i>`).join("");
  return `<span class="hud-refill${v.item.magazine <= 6 ? " is-few" : ""}">${marks}</span>`;
}

const RELOAD_DEFAULT = { now: "bar", compact: "ring", big: "tilt", rounds: "refill", bare: "count" };
const RELOAD_NAMES = { bar: "Bar", ring: "Ring", count: "Count up", refill: "Refill", tilt: "Tilt" };

const RELOAD_AREA = {
  bar: (v) => (v.type === "now" ? hudAmmo(v) : dashHtml(v)),
  ring: ringHtml,
  count: countHtml,
  refill: (v) => (v.type === "rounds" ? dashHtml(v) : refillHtml(v)),
  tilt: dashHtml,
};

function ammoArea(v) {
  if (v.ammo === null) return hudTitle(v);
  return v.reloading ? RELOAD_AREA[v.reloadStyle](v) : hudAmmo(v);
}

function hudBar(v) {
  const accent = v.reloading || isLow(v);
  const part = v.reloading && v.reloadStyle !== "bar" ? 0 : v.part;
  return `<div class="hud-track"><span class="${accent ? "is-accent" : ""}" style="width:${(part * 100).toFixed(1)}%"></span></div>`;
}

function roundTick(i, filled) {
  const classes = [i < filled ? "" : "is-spent", i > 0 && i % 10 === 0 ? "is-chunk" : ""].filter(Boolean).join(" ");
  return `<i class="${classes}"></i>`;
}

function roundsHtml(v) {
  if (v.ammo === null) return `<div class="hud-rounds"></div>`;
  const refill = v.reloadStyle === "refill" ? Math.floor(v.part * v.item.magazine) : 0;
  const filled = v.reloading ? refill : v.ammo;
  const ticks = Array.from({ length: v.item.magazine }, (_, i) => roundTick(i, filled)).join("");
  const classes = ["hud-rounds", v.item.magazine <= 6 ? "is-few" : "", v.reloading || isLow(v) ? "is-accent" : ""].filter(Boolean).join(" ");
  return `<div class="${classes}">${ticks}</div>`;
}

function hudChip(v, slot, inner) {
  const on = slot === v.slot;
  return `<button class="hud-chip${on ? " is-on" : ""}" data-hud-slot="${slot}">${hudKey(slot, on)}${inner}</button>`;
}

function stripHtml(v) {
  return v.items.map((item, slot) => hudChip(v, slot, hudImg(item, "hud-stripimg"))).join("");
}

const HUD_TYPES = {
  now: (v) =>
    hudGrenadeChip(v.state, v.grenades) +
    `<div class="hud-row hud-end">${v.items.map((item, slot) => hudChip(v, slot, `<span class="hud-name">${item.name}</span>`)).join("")}</div>` +
    `<div class="hud-row hud-between">${hudTitle(v)}${ammoArea(v)}</div>` +
    hudBar(v),
  compact: (v) =>
    `<div class="hud-main">${grenadeIcons(v.state, v.grenades)}<span class="hud-sep"></span>${hudImg(v.item, "hud-cimg hud-gunimg")}<span class="hud-right">${ammoArea(v)}</span></div>${hudBar(v)}`,
  big: (v) => {
    const sub = v.item.sightName ? `<span class="hud-sub">${v.item.sightName}</span>` : "";
    const side = `<div class="hud-hero-side">${hudTitle(v)}${sub}${ammoArea(v)}</div>`;
    return `<div class="hud-row hud-end">${grenadeIcons(v.state, v.grenades)}</div><div class="hud-hero">${hudImg(v.item, "hud-hero-img hud-gunimg")}${side}</div>${hudBar(v)}`;
  },
  rounds: (v) =>
    `<div class="hud-row hud-between">${grenadeCount(v.state, v.grenades)}<div class="hud-row">${hudImg(v.item, "hud-rimg hud-gunimg")}${ammoArea(v)}</div></div>${roundsHtml(v)}`,
  bare: (v) =>
    `<div class="hud-row hud-end">${grenadeIcons(v.state, v.grenades)}${hudTitle(v)}</div><div class="hud-row hud-end">${ammoArea(v)}</div>${hudBar(v)}`,
};

function createHud(root, store) {
  const hud = { type: "now", slot: 1, ammo: {}, reload: 0, reloadTotal: 1, grenades: 0, kind: -1, draw: 0, cooldown: 0, trigger: false, pressShot: false, lastAction: 0, kick: false, slide: false, pulse: -1, reloadStyle: "auto" };
  const strip = document.createElement("div");
  strip.className = "hud-strip";
  root.closest(".screen").append(strip);

  const now = () => performance.now() / 1000;
  const current = () => hudItem(store.get(), hud.slot);
  const touch = () => {
    hud.lastAction = now();
  };

  function ammoOf(item) {
    return item.magazine ? hud.ammo[item.id] ?? item.magazine : null;
  }

  function barPart(item) {
    if (!item.magazine) return 0;
    if (hud.reload > 0) return 1 - hud.reload / hud.reloadTotal;
    return ammoOf(item) / item.magazine;
  }

  function syncGrenadeKind(state) {
    if (hud.kind === state.grenade) return;
    hud.kind = state.grenade;
    hud.grenades = GRENADES[state.grenade].max;
  }

  function isIdle(v) {
    const quiet = now() - hud.lastAction > IDLE_AFTER && !hud.trigger;
    return hud.type !== "now" && quiet && !v.reloading && !isLow(v);
  }

  function view() {
    const state = store.get();
    syncGrenadeKind(state);
    const item = current();
    const items = [0, 1, 2].map((slot) => hudItem(state, slot));
    const reloadStyle = hud.reloadStyle === "auto" ? RELOAD_DEFAULT[hud.type] : hud.reloadStyle;
    const v = { state, item, items, slot: hud.slot, type: hud.type, ammo: ammoOf(item), reloading: hud.reload > 0, reloadLeft: hud.reload, reloadStyle, part: barPart(item), grenades: hud.grenades };
    v.idle = isIdle(v);
    return v;
  }

  function snapshot(v) {
    return [v.ammo, v.slot, Math.round(v.part * 60), Math.floor(v.grenades * 20), v.idle, hud.type, v.reloadStyle].join("|");
  }

  function render() {
    const v = view();
    hud.shown = snapshot(v);
    v.items.forEach((item) => hudImg(item, ""));
    const tilting = v.reloading && v.reloadStyle === "tilt";
    const swing = Math.sin(Math.PI * v.part);
    root.style.setProperty("--tilt", `${(swing * 18).toFixed(1)}deg`);
    root.style.setProperty("--drop", `${(swing * 6).toFixed(1)}px`);
    root.style.setProperty("--shrink", (1 - swing * 0.14).toFixed(3));
    root.className = `hud hud-type-${hud.type}${v.idle ? " is-idle" : ""}${tilting ? " is-tilting" : ""}`;
    root.parentElement.dataset.type = hud.type;
    root.innerHTML = HUD_TYPES[hud.type](v);
    return v;
  }

  function kickEffect() {
    const number = root.querySelector(".hud-ammo b");
    if (number) number.animate([{ transform: "scale(1.22)" }, { transform: "scale(1)" }], { duration: 140, easing: "ease-out" });
    const spent = root.querySelectorAll(".hud-rounds i")[ammoOf(current())];
    if (spent) spent.animate([{ background: "#ffffff", transform: "scaleY(1.6)" }, { transform: "scaleY(1)" }], { duration: 220 });
  }

  function slideEffect() {
    const image = root.querySelector(".hud-gunimg");
    if (image) image.animate([{ transform: "translateX(28px)", opacity: 0 }, { transform: "none", opacity: 1 }], { duration: 200, easing: "ease-out" });
  }

  function pulseEffect() {
    const icons = root.querySelectorAll(".hud-gicon");
    const icon = icons[hud.pulse] || root.querySelector(".hud-gcount, .hud-pips");
    if (icon) icon.animate([{ transform: "scale(1.5)", filter: "brightness(2)" }, { transform: "scale(1)" }], { duration: 320 });
  }

  function playEffects() {
    if (hud.kick) kickEffect();
    if (hud.slide) slideEffect();
    if (hud.pulse >= 0) pulseEffect();
    hud.kick = false;
    hud.slide = false;
    hud.pulse = -1;
  }

  function show() {
    render();
    playEffects();
  }

  function startReload(item) {
    hud.reloadTotal = item.reload;
    hud.reload = item.reload;
  }

  function stepReload() {
    hud.reload -= HUD_TICK;
    if (hud.reload > 0) return;
    const item = current();
    hud.ammo[item.id] = item.magazine;
    hud.reload = 0;
  }

  function stepGrenades() {
    const grenade = GRENADES[store.get().grenade];
    if (hud.grenades >= grenade.max) return;
    const before = Math.floor(hud.grenades);
    hud.grenades = Math.min(grenade.max, hud.grenades + HUD_TICK / grenade.recharge);
    if (Math.floor(hud.grenades) > before) hud.pulse = before;
  }

  function canFire() {
    return hud.cooldown <= 0 && hud.draw <= 0 && hud.reload <= 0;
  }

  function tryFire() {
    const item = current();
    if (!canFire() || (!item.auto && hud.pressShot)) return false;
    hud.pressShot = true;
    hud.cooldown = Math.max(hud.cooldown, -HUD_TICK) + item.interval;
    touch();
    if (!item.magazine) return true;
    const ammo = ammoOf(item) - 1;
    hud.ammo[item.id] = ammo;
    hud.kick = true;
    if (ammo === 0) startReload(item);
    return true;
  }

  function fireBurst() {
    for (let shots = 0; shots < 3 && tryFire(); shots += 1);
  }

  function tick() {
    hud.cooldown = Math.max(hud.trigger ? -HUD_TICK : 0, hud.cooldown - HUD_TICK);
    hud.draw = Math.max(0, hud.draw - HUD_TICK);
    if (hud.reload > 0) stepReload();
    stepGrenades();
    if (hud.trigger) fireBurst();
    if (snapshot(view()) !== hud.shown || hud.kick || hud.pulse >= 0) show();
  }

  function showStrip() {
    strip.classList.remove("is-shown");
    if (hud.type === "now") return;
    strip.innerHTML = stripHtml(view());
    const above = root.offsetHeight + 6;
    strip.style.setProperty("--strip-bottom", `${18 + above}px`);
    void strip.offsetWidth;
    strip.classList.add("is-shown");
  }

  function reload() {
    const item = current();
    if (!item.magazine || hud.reload > 0 || ammoOf(item) === item.magazine) return false;
    startReload(item);
    touch();
    show();
    return true;
  }

  function select(slot) {
    hud.reload = 0;
    hud.slot = slot;
    hud.draw = DRAW_TIME;
    hud.slide = true;
    touch();
    show();
    showStrip();
    return true;
  }

  function throwGrenade() {
    if (hud.grenades < 1) return false;
    hud.grenades -= 1;
    touch();
    show();
    return true;
  }

  const keys = { r: reload, g: throwGrenade };

  function key(name) {
    const slot = HUD_KEYS.indexOf(name);
    if (slot >= 0) return select(slot);
    const action = keys[name.toLowerCase()];
    return action ? action() : false;
  }

  function trigger(down) {
    hud.trigger = down;
    if (!down) return;
    hud.pressShot = false;
    tryFire();
    show();
  }

  root.addEventListener("click", (event) => {
    const chip = event.target.closest("[data-hud-slot]");
    if (chip) select(Number(chip.dataset.hudSlot));
  });
  store.subscribe(() => {
    touch();
    show();
  });
  document.addEventListener("gun-thumb", render);
  document.addEventListener("gunviews-ready", render);
  setInterval(tick, HUD_TICK * 1000);
  touch();
  render();
  return {
    key,
    trigger,
    setReload(style) {
      hud.reloadStyle = style;
      touch();
      render();
    },
    setType(type) {
      hud.type = type;
      strip.classList.remove("is-shown");
      touch();
      render();
    },
  };
}
