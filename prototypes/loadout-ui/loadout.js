const SLOT_KEYS = ["primary", "secondary"];
const STORE_KEY = "shitshooter-loadout";

function otherSlot(slotKey) {
  return slotKey === "primary" ? "secondary" : "primary";
}

function sightOn(gunId, wanted) {
  return GUNS[gunId].sights.includes(wanted) ? wanted : IRON_SIGHTS;
}

function sightFor(state, gunId) {
  return sightOn(gunId, state.sights[gunId] ?? IRON_SIGHTS);
}

function sightChoices(gunId) {
  return [IRON_SIGHTS, ...GUNS[gunId].sights];
}

function withGun(state, slotKey, gunId) {
  const next = { ...state, [slotKey]: gunId };
  const otherKey = otherSlot(slotKey);
  if (state[otherKey] === gunId) next[otherKey] = state[slotKey];
  return next;
}

function withSight(state, gunId, sightId) {
  const sights = state.sights.slice();
  sights[gunId] = sightOn(gunId, sightId);
  return { ...state, sights };
}

function withGrenade(state, kind) {
  return { ...state, grenade: kind };
}

function wrap(index, count) {
  return ((index % count) + count) % count;
}

function stepGun(state, slotKey, step) {
  const other = state[otherSlot(slotKey)];
  let next = wrap(state[slotKey] + step, GUNS.length);
  if (next === other) next = wrap(next + step, GUNS.length);
  return { ...state, [slotKey]: next };
}

function stepSight(state, slotKey, step) {
  const gun = state[slotKey];
  const choices = sightChoices(gun);
  const index = choices.indexOf(sightFor(state, gun));
  return withSight(state, gun, choices[wrap(index + step, choices.length)]);
}

function stepGrenade(state, step) {
  return withGrenade(state, wrap(state.grenade + step, GRENADES.length));
}

function defaultLoadout() {
  return { ...DEFAULT_LOADOUT, sights: DEFAULT_LOADOUT.sights.slice() };
}

function isIndex(value, count) {
  return Number.isInteger(value) && value >= 0 && value < count;
}

function validLoadout(saved) {
  if (!saved || !Array.isArray(saved.sights) || saved.sights.length !== GUNS.length) return false;
  const gunsOk = isIndex(saved.primary, GUNS.length) && isIndex(saved.secondary, GUNS.length);
  return gunsOk && saved.primary !== saved.secondary && isIndex(saved.grenade, GRENADES.length);
}

function loadLoadout() {
  try {
    const saved = JSON.parse(localStorage.getItem(STORE_KEY));
    return validLoadout(saved) ? saved : defaultLoadout();
  } catch {
    return defaultLoadout();
  }
}

function saveLoadout(state) {
  try {
    localStorage.setItem(STORE_KEY, JSON.stringify(state));
  } catch {
    return;
  }
}

function createStore() {
  let state = loadLoadout();
  const listeners = new Set();
  return {
    get: () => state,
    set(next) {
      state = next;
      saveLoadout(next);
      listeners.forEach((listener) => listener(state));
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

function gunImg(state, gunId, cls) {
  return imgHtml("gun", gunId, sightFor(state, gunId), cls, GUNS[gunId].name);
}

function grenadeImg(kind, cls) {
  return imgHtml("grenade", kind, 0, cls, GRENADES[kind].name);
}

function imgHtml(kind, id, sight, cls, name) {
  const url = window.GunViews ? window.GunViews.thumb(kind, id, sight) : null;
  return url ? `<img class="${cls}" src="${url}" alt="${name}">` : `<span class="${cls} lt-img-wait"></span>`;
}

function capHtml(slotKey, extra) {
  return `<div class="lt-cap"><kbd>${BINDS[slotKey]}</kbd><span>${slotKey}</span>${extra || ""}</div>`;
}

function sightLabel(state, gunId) {
  return GUNS[gunId].sights.length ? SIGHTS[sightFor(state, gunId)].name : "No sight mount";
}

function chipsHtml(state, slotKey) {
  const gun = state[slotKey];
  if (!GUNS[gun].sights.length) return `<div class="lt-chips"><span class="lt-dim">No sight mount</span></div>`;
  const chips = sightChoices(gun).map((id) => {
    const on = sightFor(state, gun) === id ? " is-on" : "";
    return `<button class="lt-chip${on}" data-act="sight" data-slot="${slotKey}" data-id="${id}" data-inspect>${SIGHTS[id].name}</button>`;
  });
  return `<div class="lt-chips">${chips.join("")}</div>`;
}

function footHtml(view) {
  if (!view.note) return "";
  const text = view.changed ? `Saved. ${view.note}` : view.note;
  return `<div class="lt-foot${view.changed ? " is-changed" : ""}">${text}</div>`;
}

function focusKey(root) {
  const el = document.activeElement;
  if (!el || !root.contains(el) || !el.dataset.act) return null;
  const parts = ["act", "slot", "id", "step"].filter((key) => el.dataset[key] !== undefined);
  return parts.map((key) => `[data-${key}="${el.dataset[key]}"]`).join("");
}

function restoreFocus(root, key) {
  const el = key && root.querySelector(key);
  if (el) el.focus();
}

function offsetIn(el, root) {
  let y = 0;
  for (let node = el; node && node !== root; node = node.offsetParent) y += node.offsetTop;
  return y;
}

function gunTarget(id, sight) {
  const sightName = GUNS[id].sights.length ? SIGHTS[sight].name : "No sight mount";
  return { kind: "gun", id, sight, label: `${GUNS[id].name} · ${sightName}` };
}

function grenadeTarget(kind) {
  return { kind: "grenade", id: kind, sight: 0, label: `${GRENADES[kind].name} grenade` };
}

function slotTarget(state, slot) {
  return slot === "grenade" ? grenadeTarget(state.grenade) : gunTarget(state[slot], sightFor(state, state[slot]));
}

function inspectTarget(state, data) {
  if (data.slot === "grenade") return grenadeTarget(data.act === "grenade" ? Number(data.id) : state.grenade);
  if (!SLOT_KEYS.includes(data.slot)) return null;
  const id = data.act === "gun" ? Number(data.id) : state[data.slot];
  return gunTarget(id, data.act === "sight" ? Number(data.id) : sightFor(state, id));
}

function createInspect() {
  const box = document.createElement("div");
  box.className = "lt-inspect";
  box.hidden = true;
  const cap = document.createElement("span");
  cap.className = "lt-inspect-cap";
  box.append(cap);
  return { box, cap, data: null, timer: 0 };
}

function mountType(def, root, store, options) {
  const view = { ...(def.view || {}), focus: "primary", note: options.note || "", changed: false };
  const inspect = createInspect();
  let viewer = null;

  function getViewer() {
    if (!viewer && window.GunViews) viewer = window.GunViews.createViewer();
    return viewer;
  }

  function placeViewer() {
    const slot = root.querySelector("[data-view]");
    if (!slot || !getViewer()) return;
    viewer.place(slot);
    viewer.show(slot.dataset.kind, Number(slot.dataset.id), Number(slot.dataset.sight));
  }

  function currentTarget() {
    const hovered = inspect.data && inspectTarget(store.get(), inspect.data);
    return hovered || (def.stage ? slotTarget(store.get(), view.focus) : null);
  }

  function refreshInspect() {
    const target = currentTarget();
    inspect.box.hidden = !target;
    if (!target || !getViewer()) return;
    inspect.cap.textContent = target.label;
    viewer.place(inspect.box);
    viewer.show(target.kind, target.id, target.sight);
  }

  function hideInspect() {
    clearTimeout(inspect.timer);
    inspect.timer = setTimeout(() => {
      inspect.data = null;
      refreshInspect();
    }, def.stage ? 350 : 900);
  }

  function placeFloating(el) {
    const y = offsetIn(el, root);
    inspect.box.style.top = `${y > 150 ? y - 128 : y + el.offsetHeight + 6}px`;
  }

  function onPoint(event) {
    const el = def.inspect && event.target.closest("[data-inspect]");
    if (!el || !root.contains(el)) return hideInspect();
    clearTimeout(inspect.timer);
    inspect.data = { ...el.dataset };
    if (!def.stage) placeFloating(el);
    refreshInspect();
  }

  function render() {
    const focus = focusKey(root);
    root.className = `lt lt-type-${def.id}`;
    root.innerHTML = `<div class="lt-section">Loadout</div>${def.html(store.get(), view)}${footHtml(view)}`;
    const dock = root.querySelector("[data-stage]");
    inspect.box.classList.toggle("is-docked", Boolean(dock));
    (dock || root).append(inspect.box);
    placeViewer();
    if (dock || !inspect.box.hidden) refreshInspect();
    restoreFocus(root, focus);
  }

  function onClick(event) {
    const el = event.target.closest("[data-act]");
    if (!el || !root.contains(el)) return;
    if (["primary", "secondary", "grenade"].includes(el.dataset.slot)) view.focus = el.dataset.slot;
    const next = def.act[el.dataset.act](store.get(), view, el.dataset);
    if (!next) return render();
    view.changed = true;
    store.set(next);
  }

  function back() {
    if (!def.back || !def.back(view)) return false;
    render();
    return true;
  }

  let awayTimer = 0;

  function onLeave() {
    hideInspect();
    clearTimeout(awayTimer);
    awayTimer = setTimeout(back, 600);
  }

  function onFocus(event) {
    if (event.target.matches(":focus-visible")) onPoint(event);
  }

  function onOutside(event) {
    if (!event.composedPath().includes(root)) back();
  }

  root.addEventListener("click", onClick);
  root.addEventListener("mouseover", onPoint);
  root.addEventListener("focusin", onFocus);
  root.addEventListener("mouseleave", onLeave);
  root.addEventListener("mouseenter", () => clearTimeout(awayTimer));
  root.addEventListener("focusout", hideInspect);
  document.addEventListener("click", onOutside);
  document.addEventListener("gun-thumb", render);
  document.addEventListener("gunviews-ready", render);
  const unsubscribe = store.subscribe(render);
  render();
  return {
    back,
    destroy() {
      unsubscribe();
      clearTimeout(awayTimer);
      document.removeEventListener("click", onOutside);
      document.removeEventListener("gun-thumb", render);
      document.removeEventListener("gunviews-ready", render);
      if (viewer) viewer.destroy();
    },
  };
}
