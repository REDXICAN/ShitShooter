const ALL_SLOTS = ["primary", "secondary", "grenade"];

const PICK = {
  gun: (state, view, data) => withGun(state, data.slot, Number(data.id)),
  sight: (state, view, data) => withSight(state, state[data.slot], Number(data.id)),
  grenade: (state, view, data) => withGrenade(state, Number(data.id)),
  tab: (state, view, data) => {
    view.slot = data.slot;
  },
};

const STEP = {
  step: (state, view, data) =>
    data.slot === "grenade" ? stepGrenade(state, Number(data.step)) : stepGun(state, data.slot, Number(data.step)),
  sightStep: (state, view, data) => stepSight(state, data.slot, Number(data.step)),
};

function classes(...names) {
  return names.filter(Boolean).join(" ");
}

function arrowsAround(act, slot, inner, small) {
  const button = (step, label) =>
    `<button class="${classes("lt-arrow", small && "is-small")}" data-act="${act}" data-slot="${slot}" data-step="${step}" data-inspect aria-label="${step < 0 ? "Previous" : "Next"}">${label}</button>`;
  return `<div class="${classes("lt-arrows", small && "is-small")}">${button(-1, "‹")}${inner}${button(1, "›")}</div>`;
}

function slotItem(state, slot) {
  if (slot === "grenade") {
    const grenade = GRENADES[state.grenade];
    return { kind: "grenade", id: state.grenade, sight: 0, name: grenade.name, sub: `${grenade.max} carried, 1 back every ${grenade.recharge} s` };
  }
  const id = state[slot];
  return { kind: "gun", id, sight: sightFor(state, id), name: GUNS[id].name, sub: sightLabel(state, id) };
}

function itemImg(state, item, cls) {
  return item.kind === "grenade" ? grenadeImg(item.id, cls) : gunImg(state, item.id, cls);
}

function tabsHtml(state, active) {
  return `<div class="lt-tabs">${ALL_SLOTS.map((slot) => {
    const on = active === slot ? " is-on" : "";
    return `<button class="lt-tab${on}" data-act="tab" data-slot="${slot}"><kbd>${BINDS[slot]}</kbd>${slotItem(state, slot).name}</button>`;
  }).join("")}</div>`;
}

function underHtml(state, slot) {
  return slot === "grenade" ? `<div class="lt-chips"><span class="lt-dim">${slotItem(state, slot).sub}</span></div>` : chipsHtml(state, slot);
}

function choices(state, slot) {
  if (slot === "grenade") {
    return GRENADES.map((grenade, id) => ({ act: "grenade", id, name: grenade.name, on: state.grenade === id, other: false, img: grenadeImg(id, "lt-choice-img") }));
  }
  const otherKey = otherSlot(slot);
  return GUNS.map((gun, id) => ({
    act: "gun",
    id,
    name: gun.name,
    on: state[slot] === id,
    other: state[otherKey] === id ? BINDS[otherKey] : false,
    img: gunImg(state, id, "lt-choice-img"),
  }));
}

function badgeHtml(choice) {
  return choice.other ? `<span class="lt-badge" title="In slot ${choice.other}: picking it swaps the two">${choice.other}</span>` : "";
}

const TYPE_NOW = {
  id: "now",
  html(state) {
    const sight = (slot) => (GUNS[state[slot]].sights.length ? `Sight: ${sightLabel(state, state[slot])}` : "No sight mount");
    return `<button class="now-btn" data-act="gun" data-slot="primary">Primary: ${GUNS[state.primary].name}</button>
      <button class="now-btn is-small" data-act="sight" data-slot="primary">${sight("primary")}</button>
      <button class="now-btn" data-act="gun" data-slot="secondary">Secondary: ${GUNS[state.secondary].name}</button>
      <button class="now-btn is-small" data-act="sight" data-slot="secondary">${sight("secondary")}</button>
      <button class="now-btn" data-act="grenade">Grenade: ${GRENADES[state.grenade].name}</button>`;
  },
  act: {
    gun: (state, view, data) => stepGun(state, data.slot, 1),
    sight: (state, view, data) => stepSight(state, data.slot, 1),
    grenade: (state) => stepGrenade(state, 1),
  },
};

function arrowsSlot(state, slot) {
  const item = slotItem(state, slot);
  const text = `<span class="t1-text">${capHtml(slot)}<span class="t1-name">${item.name}</span></span>`;
  const main = `<div class="t1-item">${text}${itemImg(state, item, "t1-img")}</div>`;
  const hasSights = slot !== "grenade" && GUNS[item.id].sights.length;
  const sight = hasSights ? arrowsAround("sightStep", slot, `<span class="t1-sight">${item.sub}</span>`, true) : "";
  const none = slot !== "grenade" && !hasSights ? `<div class="t1-none lt-dim">No sight mount</div>` : "";
  return `<div class="t1-slot">${arrowsAround("step", slot, main, false)}${sight}${none}</div>`;
}

const STAGE_HTML = `<div class="lt-stage" data-stage></div>`;

const TYPE_ARROWS = {
  id: "arrows",
  inspect: true,
  stage: true,
  html: (state) => STAGE_HTML + ALL_SLOTS.map((slot) => arrowsSlot(state, slot)).join(""),
  act: STEP,
};

const TYPE_PREVIEW = {
  id: "preview",
  view: { slot: "primary" },
  html(state, view) {
    const item = slotItem(state, view.slot);
    const stage = `<div class="t3-view" data-view data-kind="${item.kind}" data-id="${item.id}" data-sight="${item.sight}"></div>`;
    const pick = arrowsAround("step", view.slot, `<span class="t3-name">${item.name}</span>`, false);
    return `${tabsHtml(state, view.slot)}${stage}${pick}${underHtml(state, view.slot)}`;
  },
  act: { ...PICK, ...STEP },
};

function cardHtml(state, slot) {
  const item = slotItem(state, slot);
  return `<button class="t4-card" data-act="open" data-slot="${slot}" data-inspect>
    ${capHtml(slot)}
    <span class="t4-name">${item.name}</span>
    <span class="t4-sub">${slot === "grenade" ? `${GRENADES[item.id].max} carried` : item.sub}</span>
    ${itemImg(state, item, "t4-img")}
  </button>`;
}

function popupRow(choice, slot) {
  const cls = classes("t4-row", choice.on && "is-on", choice.other && "is-other");
  return `<button class="${cls}" data-act="${choice.act}" data-slot="${slot}" data-id="${choice.id}" data-inspect>${choice.img}<span class="t4-row-name">${choice.name}</span>${badgeHtml(choice)}</button>`;
}

function popupHtml(state, slot) {
  const rows = choices(state, slot).map((choice) => popupRow(choice, slot)).join("");
  const sights = slot === "grenade" ? "" : chipsHtml(state, slot);
  return `<div class="t4-shade" data-act="close"></div>
    <div class="t4-pop">
      <div class="t4-pop-head">${capHtml(slot)}<button class="t4-x" data-act="close" aria-label="Close (Esc)">×</button></div>
      <div class="t4-rows">${rows}</div>${sights}
    </div>`;
}

const TYPE_CARDS = {
  id: "cards",
  inspect: true,
  stage: true,
  view: { popup: null },
  html: (state, view) =>
    `${STAGE_HTML}<div class="t4-cards">${ALL_SLOTS.map((slot) => cardHtml(state, slot)).join("")}</div>${view.popup ? popupHtml(state, view.popup) : ""}`,
  act: {
    ...PICK,
    open: (state, view, data) => {
      view.popup = data.slot;
    },
    close: (state, view) => {
      view.popup = null;
    },
  },
  back(view) {
    if (!view.popup) return false;
    view.popup = null;
    return true;
  },
};

const LOADOUT_TYPES = {
  now: TYPE_NOW,
  arrows: TYPE_ARROWS,
  preview: TYPE_PREVIEW,
  cards: TYPE_CARDS,
};
