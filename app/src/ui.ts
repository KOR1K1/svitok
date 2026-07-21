// DOM-хелперы, иконки, вибро-отклик, тосты.

type Attrs = Record<string, string | number | boolean | ((e: Event) => void)>;

/** Коротко собираем элемент: h("div.row", {..}, [children]) */
export function h(tag: string, attrs: Attrs = {}, children: (Node | string)[] = []): HTMLElement {
  const [name, ...classes] = tag.split(".");
  const el = document.createElement(name || "div");
  if (classes.length) el.className = classes.join(" ");
  for (const [k, v] of Object.entries(attrs)) {
    if (k.startsWith("on") && typeof v === "function") {
      el.addEventListener(k.slice(2).toLowerCase(), v as EventListener);
    } else if (k === "class") {
      el.className += " " + v;
    } else if (typeof v === "boolean") {
      if (v) el.setAttribute(k, "");
    } else {
      el.setAttribute(k, String(v));
    }
  }
  for (const c of children) el.append(c);
  return el;
}

export function clear(el: HTMLElement) {
  while (el.firstChild) el.removeChild(el.firstChild);
}

// ---------- Иконки (один набор, штрих 1.6) ----------

export function svgEl(inner: string, cls: string, viewBox = "0 0 24 24"): SVGElement {
  const s = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  s.setAttribute("viewBox", viewBox);
  s.setAttribute("class", cls);
  // Круглые концы и стыки, как в Lucide. Без них штрих выглядит рубленым.
  s.setAttribute("stroke-linecap", "round");
  s.setAttribute("stroke-linejoin", "round");
  s.innerHTML = inner;
  return s;
}
function svg(paths: string, cls = "tab__icon"): SVGElement {
  return svgEl(paths, cls);
}
/** Наш логотип-свиток: спокойный лайн-арт в цвет темы. */
export function logoScroll(extraClass = ""): SVGElement {
  return svgEl(
    '<ellipse cx="16" cy="32" rx="6" ry="15"/>' +
      '<ellipse cx="48" cy="32" rx="6" ry="15"/>' +
      '<path d="M16 17 H48"/><path d="M16 47 H48"/>' +
      '<g class="logo__lines" stroke-width="2.3">' +
      '<path d="M25 27 H39"/><path d="M25 32 H39"/><path d="M25 37 H39"/>' +
      "</g>",
    ("logo " + extraClass).trim(),
    "0 0 64 64"
  );
}

// Контуры взяты из набора Lucide (ISC license, https://lucide.dev) и
// вшиты прямо сюда, чтобы не тянуть зависимость в рантайме.
// viewBox 24, толщину штриха задаёт CSS.
export const icons = {
  sites: () => svg('<path d="M3 5h.01"/><path d="M3 12h.01"/><path d="M3 19h.01"/><path d="M8 5h13"/><path d="M8 12h13"/><path d="M8 19h13"/>'),
  codes: () => svg('<circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/>'),
  vault: () => svg('<rect width="18" height="18" x="3" y="3" rx="2"/><circle cx="7.5" cy="7.5" r=".5" fill="currentColor"/><path d="m7.9 7.9 2.7 2.7"/><circle cx="16.5" cy="7.5" r=".5" fill="currentColor"/><path d="m13.4 10.6 2.7-2.7"/><circle cx="7.5" cy="16.5" r=".5" fill="currentColor"/><path d="m7.9 16.1 2.7-2.7"/><circle cx="16.5" cy="16.5" r=".5" fill="currentColor"/><path d="m13.4 13.4 2.7 2.7"/><circle cx="12" cy="12" r="2"/>'),
  chev: () => svg('<path d="m9 18 6-6-6-6"/>', "row__chev tab__icon"),
  eye: () => svg('<path d="M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0"/><circle cx="12" cy="12" r="3"/>', ""),
  eyeOff: () => svg('<path d="M10.733 5.076a10.744 10.744 0 0 1 11.205 6.575 1 1 0 0 1 0 .696 10.747 10.747 0 0 1-1.444 2.49"/><path d="M14.084 14.158a3 3 0 0 1-4.242-4.242"/><path d="M17.479 17.499a10.75 10.75 0 0 1-15.417-5.151 1 1 0 0 1 0-.696 10.75 10.75 0 0 1 4.446-5.143"/><path d="m2 2 20 20"/>', ""),
  paste: () => svg('<path d="M11 14h10"/><path d="M16 4h2a2 2 0 0 1 2 2v1.344"/><path d="m17 18 4-4-4-4"/><path d="M8 4H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 1.793-1.113"/><rect x="8" y="2" width="8" height="4" rx="1"/>', ""),
  copy: () => svg('<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>', ""),
  key: () => svg('<path d="M2.586 17.414A2 2 0 0 0 2 18.828V21a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h1a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h.172a2 2 0 0 0 1.414-.586l.814-.814a6.5 6.5 0 1 0-4-4z"/><circle cx="16.5" cy="7.5" r=".5" fill="currentColor"/>', "btn-i"),
  shield: () => svg('<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/><path d="m9 12 2 2 4-4"/>', "btn-i"),
  ticket: () => svg('<path d="M2 9a3 3 0 0 1 0 6v2a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-2a3 3 0 0 1 0-6V7a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2Z"/><path d="M13 5v2"/><path d="M13 17v2"/><path d="M13 11v2"/>', "btn-i"),
  note: () => svg('<path d="M21 9a2.4 2.4 0 0 0-.706-1.706l-3.588-3.588A2.4 2.4 0 0 0 15 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2z"/><path d="M15 3v5a1 1 0 0 0 1 1h5"/>', "btn-i"),
  doc: () => svg('<path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z"/><path d="M14 2v5a1 1 0 0 0 1 1h5"/><path d="M10 9H8"/><path d="M16 13H8"/><path d="M16 17H8"/>', "btn-i"),
  gear: () => svg('<path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915"/><circle cx="12" cy="12" r="3"/>', "btn-i"),
  lock: () => svg('<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>', "btn-i"),
  save: () => svg('<path d="M12 15V3"/><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/>', "btn-i"),
  info: () => svg('<circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/>', "btn-i"),
  camera: () => svg('<path d="M13.997 4a2 2 0 0 1 1.76 1.05l.486.9A2 2 0 0 0 18.003 7H20a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V9a2 2 0 0 1 2-2h1.997a2 2 0 0 0 1.759-1.048l.489-.904A2 2 0 0 1 10.004 4z"/><circle cx="12" cy="13" r="3"/>', "btn-i"),
  qr: () => svg('<rect width="5" height="5" x="3" y="3" rx="1"/><rect width="5" height="5" x="16" y="3" rx="1"/><rect width="5" height="5" x="3" y="16" rx="1"/><path d="M21 16h-3a2 2 0 0 0-2 2v3"/><path d="M21 21v.01"/><path d="M12 7v3a2 2 0 0 1-2 2H7"/><path d="M3 12h.01"/><path d="M12 3h.01"/><path d="M12 16v.01"/><path d="M16 12h1"/><path d="M21 12v.01"/><path d="M12 21v-1"/>', "btn-i"),
  edit: () => svg('<path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z"/><path d="m15 5 4 4"/>', "btn-i"),
  del: () => svg('<path d="M10 11v6"/><path d="M14 11v6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/><path d="M3 6h18"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>', "btn-i"),
  bump: () => svg('<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/>', "btn-i"),
};

/** Немного конфетти на успех. Только transform; при reduced-motion не показываем. */
export function confetti() {
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  const colors = ["#d4643e", "#8fa968", "#c99a4b", "#ede7de"];
  const layer = document.createElement("div");
  layer.className = "confetti";
  for (let i = 0; i < 44; i++) {
    const p = document.createElement("i");
    p.style.left = Math.random() * 100 + "vw";
    p.style.background = colors[i % colors.length];
    p.style.setProperty("--dur", 1900 + Math.random() * 1300 + "ms");
    p.style.setProperty("--x", Math.random() * 140 - 70 + "px");
    p.style.setProperty("--rot", Math.random() * 720 - 360 + "deg");
    p.style.animationDelay = Math.random() * 250 + "ms";
    layer.append(p);
  }
  document.body.append(layer);
  setTimeout(() => layer.remove(), 3600);
}

// ---------- Вибро-отклик ----------
// Сейчас идём через navigator.vibrate (Android). В планах - нативный мост
// performHapticFeedback (VIRTUAL_KEY/CONFIRM/REJECT) через Kotlin-плагин.

export type Haptic = "tap" | "confirm" | "reject" | "long";
const HAPTIC: Record<Haptic, number | number[]> = {
  tap: 8,
  confirm: [12, 40, 12],
  reject: [24, 30, 24, 30, 24],
  long: 18,
};
export function haptic(kind: Haptic) {
  try { if (localStorage.getItem("svitok.haptics") === "0") return; } catch { /* ну и ладно */ }
  const nav = navigator as Navigator & { vibrate?: (p: number | number[]) => boolean };
  // Именно как метод navigator, иначе на Android ловим TypeError: Illegal invocation.
  if (typeof nav.vibrate === "function") {
    try { nav.vibrate(HAPTIC[kind]); } catch { /* не критично */ }
  }
}

// ---------- Тост ----------

let toastEl: HTMLElement | null = null;
let toastTimer = 0;
export function toast(msg: string, kind: "ok" | "err" | "" = "") {
  if (!toastEl) {
    toastEl = h("div", {
      style:
        "position:absolute;left:50%;bottom:calc(var(--tabbar-h) + var(--sab) + 24px);transform:translateX(-50%);" +
        "background:var(--surface-2);border:1px solid var(--line);border-radius:12px;padding:10px 16px;" +
        "font-size:14px;z-index:50;max-width:80%;text-align:center;transition:opacity .2s;pointer-events:none;",
    });
    document.body.append(toastEl);
  }
  toastEl.textContent = msg;
  toastEl.style.color = kind === "err" ? "var(--err)" : kind === "ok" ? "var(--ok)" : "var(--text)";
  toastEl.style.opacity = "1";
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => { if (toastEl) toastEl.style.opacity = "0"; }, 2200);
}

/** Режем секрет на группы по 4 символа, чтобы читалось глазами. */
export function groupSecret(s: string, size = 4): string {
  const out: string[] = [];
  for (let i = 0; i < s.length; i += size) out.push(s.slice(i, i + size));
  return out.join(" ");
}
