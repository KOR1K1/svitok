import { api, clipboardRead, clipboardWrite, clipCopy, clipClear, type SiteView, type EntryView } from "./api";
import { h, clear, icons, haptic, toast, groupSecret, svgEl, logoScroll, confetti } from "./ui";
import { t, getLang, setLang } from "./i18n";
import { IS_MOBILE, scanQr, parseOtpauth } from "./scan";
import "./styles.css";

const app = document.getElementById("app")!;

// Разметка зависит от устройства: на ПК сайдбар и горячие клавиши, на телефоне
// нижние вкладки и edge-to-edge. Дизайн один и тот же.
document.body.classList.add(IS_MOBILE ? "is-mobile" : "is-desktop");

// корневой роутер

/** Сплеш. Промис резолвится, когда прошло минимальное время показа. */
function showSplash(): Promise<void> {
  setScreen(
    h("div.splash", {}, [logoScroll("logo--lg"), h("div.wordmark.wordmark--muted", {}, [t("app.name")])])
  );
  return new Promise((res) => setTimeout(res, 1000));
}

/** Строка сида: номер («01»/«==») красим в цвет темы, группы не переносим. */
function paperLine(l: string): HTMLElement {
  const sp = l.indexOf(" ");
  if (sp < 0) return h("div.paper-line", {}, [h("span.paper-n", {}, [l])]);
  return h("div.paper-line", {}, [h("span.paper-n", {}, [l.slice(0, sp)]), " " + l.slice(sp + 1)]);
}

// Защита экрана: на экране блокировки всегда включена (там вводят фразу и
// показывают сид), а сохранённую настройку пользователя применяем только
// после разблокировки - снять защиту команда даёт лишь с мастер-ключом.
function applyScreenProtectPref() {
  let off = false;
  try { off = localStorage.getItem("svitok.screenProtect") === "0"; } catch { off = false; }
  api.setScreenProtection(!off).catch(() => {});
}

// Заголовок окна на десктопе следует за языком: в тайтлбаре «Свиток» или «Svitok».
async function syncWindowTitle() {
  if (IS_MOBILE) return;
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().setTitle(t("app.name"));
  } catch { /* окна нет - не десктоп */ }
}

async function boot() {
  const minShown = showSplash();
  syncWindowTitle();
  try {
    const st = await api.status();
    await minShown;
    const onboarded = (() => { try { return localStorage.getItem("svitok.onboarded") === "1"; } catch { return false; } })();
    // Есть сид - разблокировка. Список есть, а сида нет (восстановили из бэкапа
    // или другое устройство) - просим ввести сид. Иначе создаём новый.
    const proceed = () => {
      if (st.hasSeed) return screenUnlock();
      if (st.hasVault) return screenRestore();
      return screenCreate();
    };
    if (!onboarded) return screenIntro(proceed);
    return proceed();
  } catch (e) {
    await minShown;
    setScreen(h("div.empty", {}, [t("err.core", { e: String(e) })]));
  }
}

// онбординг-тур

function screenIntro(onDone: () => void) {
  screenBack = null;
  const slides = [
    { title: t("onb.s1.title"), body: t("onb.s1.body") },
    { title: t("onb.s2.title"), body: t("onb.s2.body") },
    { title: t("onb.s3.title"), body: t("onb.s3.body") },
    { title: t("onb.s4.title"), body: t("onb.s4.body") },
    { title: t("onb.s5.title"), body: t("onb.s5.body") },
    { title: t("onb.s6.title"), body: t("onb.s6.body") },
  ];
  let idx = 0;
  const track = h("div.intro__track", {},
    slides.map((s, i) => h("div.intro__slide", {}, [
      logoScroll("logo--lg"),
      // Первый слайд - бренд «Свиток», тот же вордмарк (Golos), что на сплеше
      // и экранах входа. У остальных слайдов заголовочный шрифт.
      i === 0
        ? h("div.wordmark", { style: "margin-top:8px" }, [s.title])
        : h("div.t-title", { style: "margin-top:8px" }, [s.title]),
      h("div.t-body-2", {}, [s.body]),
    ]))
  );
  const dots = h("div.intro__dots", {}, slides.map(() => h("div.intro__dot")));
  const next = h("button.btn.btn--seal", { style: "flex:2" }, [t("onb.next")]);
  const skip = h("button.btn.btn--ghost", { style: "flex:1" }, [t("onb.skip")]);

  const finish = () => {
    try { localStorage.setItem("svitok.onboarded", "1"); } catch {}
    onDone();
  };
  const last = () => idx === slides.length - 1;
  const update = () => {
    track.style.transform = `translateX(${-idx * 100}%)`;
    Array.from(dots.children).forEach((d, i) => (d as HTMLElement).classList.toggle("intro__dot--on", i === idx));
    next.textContent = last() ? t("onb.start") : t("onb.next");
    skip.style.display = last() ? "none" : "";
  };
  next.addEventListener("click", () => {
    if (idx < slides.length - 1) { idx++; update(); haptic("tap"); } else finish();
  });
  skip.addEventListener("click", finish);

  const vp = h("div.intro__vp", {}, [track]);
  let sx = 0, dragging = false;
  vp.addEventListener("pointerdown", (e) => { sx = (e as PointerEvent).clientX; dragging = true; });
  vp.addEventListener("pointerup", (e) => {
    if (!dragging) return;
    dragging = false;
    const dx = (e as PointerEvent).clientX - sx;
    if (dx < -40 && idx < slides.length - 1) { idx++; update(); haptic("tap"); }
    else if (dx > 40 && idx > 0) { idx--; update(); haptic("tap"); }
  });

  // язык можно выбрать прямо в туре
  const mkLang = (l: "ru" | "en", label: string) => {
    const c = h("button.intro__langchip", {}, [label]);
    if (getLang() === l) c.classList.add("intro__langchip--on");
    c.addEventListener("click", () => { if (getLang() !== l) { setLang(l); haptic("tap"); screenIntro(onDone); } });
    return c;
  };
  const langToggle = h("div.intro__lang", {}, [mkLang("ru", "RU"), mkLang("en", "EN")]);
  // Топ-бар сам держит отступ от статус-бара (минимум 38px) на случай, если --sat ещё 0.
  const topBar = h("div", {
    style: "display:flex;justify-content:center;padding:max(var(--sat),38px) 0 4px",
  }, [langToggle]);

  const btnRow = h("div", { style: "display:flex;gap:10px" }, [skip, next]);
  setScreen(h("div.screen", { style: "padding-top:0" }, [
    topBar,
    vp,
    h("div.px.stack.gap-3", { style: "padding-bottom:calc(var(--sab) + 36px)" }, [dots, btnRow]),
  ]));
  update();
}

// настройки

function segmented(options: { label: string; active: boolean; onClick: () => void }[]): HTMLElement {
  return h("div", { style: "display:flex;gap:8px;flex-wrap:wrap" }, options.map((o) => {
    const b = h("button.btn", { style: "min-height:44px;padding:0 16px;flex:1 1 auto" }, [o.label]);
    b.style.background = o.active ? "var(--seal)" : "var(--surface-2)";
    b.style.color = o.active ? "var(--on-seal)" : "var(--text)";
    b.addEventListener("click", () => { haptic("tap"); o.onClick(); });
    return b;
  }));
}

// Смена языка. На телефоне хватает перерисовки экрана настроек. На ПК настройки
// живут панелью в шелле, а сайдбар собран отдельно - поэтому пересобираем весь
// шелл (сайдбар получит новые подписи) и снова открываем настройки.
function switchLang(lang: "ru" | "en", refresh: () => void) {
  if (getLang() === lang) return;
  setLang(lang);
  syncWindowTitle();
  if (!IS_MOBILE && shellNav && shellContent) {
    screenMain();
    screenSettings();
  } else {
    refresh();
  }
}

/** Секции настроек, общие для телефона и ПК. `refresh` перерисовывает экран. */
function settingsBody(refresh: () => void): HTMLElement[] {
  const lockSec = (() => { try { return parseInt(localStorage.getItem("svitok.lockSec") || "120", 10); } catch { return 120; } })();
  const setLock = (s: number) => {
    try { localStorage.setItem("svitok.lockSec", String(s)); } catch {}
    resetIdle();
    refresh();
  };
  const bgSw = switchToggle(t("settings.lockOnBg"), lockOnBackground());
  bgSw.input.addEventListener("change", () => {
    try { localStorage.setItem("svitok.lockOnBg", bgSw.input.checked ? "1" : "0"); } catch {}
    haptic("tap");
  });
  const hOn = (() => { try { return localStorage.getItem("svitok.haptics") !== "0"; } catch { return true; } })();
  const hSw = switchToggle(t("settings.haptics"), hOn);
  hSw.input.addEventListener("change", () => {
    try { localStorage.setItem("svitok.haptics", hSw.input.checked ? "1" : "0"); } catch {}
    haptic("tap");
  });
  const pOn = (() => { try { return localStorage.getItem("svitok.screenProtect") !== "0"; } catch { return true; } })();
  const pSw = switchToggle(t("settings.screenProtect"), pOn);
  pSw.input.addEventListener("change", () => {
    const on = pSw.input.checked;
    try { localStorage.setItem("svitok.screenProtect", on ? "1" : "0"); } catch {}
    api.setScreenProtection(on).catch(() => {});
    haptic("tap");
  });
  // вибрация есть только на телефоне
  const interfaceRows = IS_MOBILE ? [hSw.el, pSw.el] : [pSw.el];

  return [
    vaultAddBtn(t("settings.howto"), () => sheetHowto(), icons.info()),
    h("div.stack.gap-3", {}, [
      h("div.t-section", {}, [t("settings.lang")]),
      segmented([
        { label: "Русский", active: getLang() === "ru", onClick: () => switchLang("ru", refresh) },
        { label: "English", active: getLang() === "en", onClick: () => switchLang("en", refresh) },
      ]),
    ]),
    h("div.stack.gap-3", {}, [
      h("div.t-section", {}, [t("settings.lock")]),
      segmented([
        { label: t("settings.lock.30s"), active: lockSec === 30, onClick: () => setLock(30) },
        { label: t("settings.lock.1m"), active: lockSec === 60, onClick: () => setLock(60) },
        { label: t("settings.lock.2m"), active: lockSec === 120, onClick: () => setLock(120) },
        { label: t("settings.lock.5m"), active: lockSec === 300, onClick: () => setLock(300) },
        { label: t("settings.lock.never"), active: lockSec === 0, onClick: () => setLock(0) },
      ]),
      bgSw.el,
      h("div.t-body-2.faint", { style: "line-height:1.5" }, [t("settings.lockHint")]),
    ]),
    h("div.stack.gap-3", {}, [
      h("div.t-section", {}, [t("settings.backup")]),
      vaultAddBtn(t("settings.backup"), () => sheetBackup(), icons.save()),
      vaultAddBtn(t("settings.sync"), () => sheetSync(), icons.qr()),
      vaultAddBtn(t("settings.showSeed"), () => sheetShowSeed(), icons.doc()),
    ]),
    h("div.stack.gap-3", {}, [h("div.t-section", {}, [t("settings.interface")]), ...interfaceRows]),
    h("div.stack.gap-2", {}, [
      h("div.t-section", {}, [t("settings.about")]),
      h("div.t-body-2", { style: "line-height:1.6" }, [t("settings.aboutBody")]),
    ]),
    (() => {
      const wrap = h("div.stack.gap-2");
      const btn = h("button.btn.btn--danger.btn--full", {}, [icons.del(), t("settings.destroy")]);
      btn.addEventListener("click", () => {
        clear(wrap);
        const yes = h("button.btn.btn--danger.btn--full", {}, [t("settings.destroyConfirm")]);
        yes.addEventListener("click", async () => {
          try { await api.destroyVault(); } catch { /* даже если что-то не удалилось - выходим на создание */ }
          try { localStorage.removeItem("svitok.coached"); localStorage.removeItem("svitok.backupStale"); } catch { /* игнор */ }
          haptic("confirm");
          screenCreate();
        });
        const no = h("button.btn.btn--ghost.btn--full", {}, [t("common.cancel")]);
        no.addEventListener("click", () => { clear(wrap); wrap.append(btn); });
        wrap.append(h("div.t-body-2", { style: "line-height:1.5" }, [t("settings.destroyAsk")]), yes, no);
      });
      wrap.append(h("div.t-section", {}, [t("settings.danger")]), btn);
      return h("div.stack.gap-2", {}, [wrap]);
    })(),
  ];
}

function screenSettings() {
  // На десктопе рисуем в контент-панель шелла: сайдбар остаётся, кнопки «назад» нет.
  if (!IS_MOBILE && shellContent && shellNav) {
    for (const el of Array.from(shellNav.children)) {
      el.classList.toggle("tab--active", (el as HTMLElement).dataset.nav === "settings");
    }
    clearInterval(tickTimer);
    clear(shellContent);
    shellContent.append(h("div.stack", { style: "height:100%" }, [
      h("div.screen__head", {}, [h("div.t-title", {}, [t("settings.title")])]),
      h("div.screen__scroll.px.stack.gap-6", {}, settingsBody(screenSettings)),
    ]));
    return;
  }
  // На телефоне это отдельный полноэкранный экран с кнопкой «назад».
  screenBack = () => screenMain();
  const back = h("button.settings-back", {}, ["‹ " + t("restore.back")]);
  back.addEventListener("click", () => screenMain());
  setScreen(h("div.screen", {}, [
    h("div.screen__head", { style: "gap:12px;align-items:center" }, [back, h("div.t-title", {}, [t("settings.title")])]),
    h("div.screen__scroll.px.stack.gap-6", {}, settingsBody(screenSettings)),
  ]));
}

// резервная копия

function sheetBackup() {
  openSheet(() => {
    const err = h("div.t-body-2.err", { style: "min-height:20px" });

    const copyBtn = h("button.btn.btn--seal.btn--full", {}, [t("backup.copy")]);
    copyBtn.addEventListener("click", async () => {
      try {
        const blob = await api.backupExport();
        await copyToClipboard(blob).catch(() => {});
        haptic("confirm");
        toast(t("backup.copied"), "ok");
        clearBackupReminder();
      } catch (e) { err.textContent = String(e); }
    });

    const paste = h("textarea.field", { placeholder: t("backup.importPh"), rows: "4" }) as HTMLTextAreaElement;
    const pasteBlock = withTools(paste, { paste: true });
    const importBtn = h("button.btn.btn--full", {}, [t("backup.import")]);
    importBtn.addEventListener("click", async () => {
      err.textContent = "";
      if (!paste.value.trim()) return;
      try {
        const n = await api.backupImport(paste.value);
        haptic("confirm");
        confetti();
        toast(t("backup.imported", { n }), "ok");
        clearBackupReminder();
      } catch (e) { err.textContent = String(e); }
    });

    return h("div.stack.gap-3", {}, [
      h("div.t-title", {}, [t("backup.title")]),
      h("div.t-body-2", { style: "line-height:1.6" }, [t("backup.exportHint")]),
      copyBtn,
      h("div.t-body-2.faint", { style: "line-height:1.6;margin-top:8px" }, [t("backup.importHint")]),
      pasteBlock,
      importBtn,
      err,
    ]);
  });
}

// перенос списка по QR

function sheetSync() {
  openSheet((close) => {
    const err = h("div.t-body-2.err", { style: "min-height:20px" });
    const holder = h("div.center", {
      style: "background:#F5F0E8;border-radius:16px;padding:8px;max-width:320px;margin:0 auto;width:100%;display:none",
    });

    // это устройство показывает свой список, другое сканирует камерой
    const showBtn = h("button.btn.btn--full", {}, [icons.doc(), t("sync.show")]);
    showBtn.addEventListener("click", async () => {
      err.textContent = "";
      try {
        const data = await api.syncExport();
        const svg = await api.qrSvg(data);
        holder.innerHTML = svg;
        const el = holder.querySelector("svg");
        if (el) { el.style.width = "100%"; el.style.height = "auto"; el.style.display = "block"; }
        holder.style.display = "block";
        showBtn.style.display = "none";
        haptic("tap");
      } catch (e) { err.textContent = String(e); }
    });

    // сканируем QR другого устройства и вливаем список (только на телефоне)
    const confirmBox = h("div.stack.gap-2", { style: "display:none" });
    const finish = (n: number) => {
      haptic("confirm");
      confetti();
      toast(t("sync.imported", { n }), "ok");
      markBackupStale();
      close();
      (document.querySelector('.tab[data-tab="sites"]') as HTMLElement | null)?.click();
    };
    const scanBtn = h("button.btn.btn--seal.btn--full", {}, [icons.camera(), t("sync.scan")]);
    scanBtn.addEventListener("click", async () => {
      err.textContent = "";
      confirmBox.style.display = "none";
      confirmBox.replaceChildren();
      haptic("tap");
      try {
        const content = await scanQr();
        if (!content) return;
        const prev = await api.syncPreview(content);
        if (prev.added.length === 0 && prev.updated.length === 0) { toast(t("sync.nothing"), "ok"); return; }
        // новые сайты добавляем всегда, перезапись существующих меняет выводимый
        // пароль - её применяем только по явному согласию, показав что затронуто
        if (prev.updated.length === 0) { finish(await api.syncImport(content, false)); return; }

        const lines: HTMLElement[] = [];
        if (prev.added.length) lines.push(h("div.t-body-2", {}, [t("sync.willAdd", { n: prev.added.length })]));
        lines.push(h("div.t-body-2.err", { style: "line-height:1.5" }, [t("sync.willUpdate", { names: prev.updated.join(", ") })]));
        const both = h("button.btn.btn--danger.btn--full", {}, [t("sync.applyBoth")]);
        both.addEventListener("click", async () => { try { finish(await api.syncImport(content, true)); } catch (e) { err.textContent = String(e); } });
        const addOnly = h("button.btn.btn--full", {}, [t("sync.addOnly")]);
        addOnly.addEventListener("click", async () => { try { finish(await api.syncImport(content, false)); } catch (e) { err.textContent = String(e); } });
        confirmBox.replaceChildren(...lines, both, addOnly);
        confirmBox.style.display = "block";
      } catch (e) {
        const msg = String(e);
        err.textContent = /no-camera/.test(msg) ? t("scan.noCamera") : msg;
      }
    });

    return h("div.stack.gap-3", {}, [
      h("div.t-title", {}, [t("sync.title")]),
      h("div.t-body-2", { style: "line-height:1.6" }, [t("sync.hint")]),
      showBtn,
      holder,
      ...(IS_MOBILE ? [h("div.t-body-2.faint", { style: "line-height:1.6;margin-top:4px" }, [t("sync.scanHint")]), scanBtn] : []),
      confirmBox,
      err,
    ]);
  });
}

// Флаг «копия устарела»: ставим после изменений списка, потом показываем баннером в Сейфе.
function markBackupStale() {
  try { localStorage.setItem("svitok.backupStale", "1"); } catch {}
}
function isBackupStale(): boolean {
  try { return localStorage.getItem("svitok.backupStale") === "1"; } catch { return false; }
}
function clearBackupReminder() {
  try { localStorage.removeItem("svitok.backupStale"); } catch {}
}

// умный тур с подсветкой (показываем после входа в хранилище)

interface CoachStep { selector: string; title: string; body: string; tab?: Tab; }

function maybeCoach() {
  const coached = (() => { try { return localStorage.getItem("svitok.coached") === "1"; } catch { return false; } })();
  if (coached) return;
  setTimeout(() => {
    if (!mainScreenActive || currentTab !== "sites") return;
    runCoach([
      { tab: "sites", selector: ".tabbar", title: t("coach.s1.title"), body: t("coach.s1.body") },
      { tab: "sites", selector: ".fab", title: t("coach.s2.title"), body: t("coach.s2.body") },
      { tab: "codes", selector: ".fab", title: t("coach.s3.title"), body: t("coach.s3.body") },
      // последний шаг подсвечивает саму вкладку «Сейф» в панели, внутрь не заходим
      { tab: "codes", selector: '.tab[data-tab="vault"]', title: t("coach.s4.title"), body: t("coach.s4.body") },
    ]);
  }, 550);
}

function cssPx(name: string): number {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name);
  return parseFloat(v) || 0;
}

function waitFor(selector: string, timeout = 900): Promise<HTMLElement | null> {
  return new Promise((resolve) => {
    const t0 = performance.now();
    const tick = () => {
      const el = document.querySelector(selector) as HTMLElement | null;
      if (el) return resolve(el);
      if (performance.now() - t0 > timeout) return resolve(null);
      requestAnimationFrame(tick);
    };
    tick();
  });
}

function runCoach(steps: CoachStep[]) {
  const overlay = h("div.coach");
  const spot = h("div.coach__spot");
  const card = h("div.coach__card");
  overlay.append(spot, card);
  document.body.append(overlay);
  let idx = 0;

  const finish = () => {
    try { localStorage.setItem("svitok.coached", "1"); } catch {}
    overlay.remove();
    // после тура возвращаемся на вкладку «Сайты»
    (document.querySelector('.tab[data-tab="sites"]') as HTMLElement | null)?.click();
  };
  const advance = () => { if (idx < steps.length - 1) { idx++; void render(); } else finish(); };

  const render = async () => {
    const step = steps[idx];
    // если надо, переключаем вкладку и ждём, пока появится цель
    if (step.tab && currentTab !== step.tab) {
      const tabBtn = document.querySelector(`.tab[data-tab="${step.tab}"]`) as HTMLElement | null;
      tabBtn?.click();
    }
    const el = await waitFor(step.selector);
    if (!el) { advance(); return; }

    const r = el.getBoundingClientRect();
    const pad = 6;
    // Зажимаем по всем краям, чтобы прожектор не лез в статус-бар, жестовую полосу и за бока.
    const vw = window.innerWidth, vh = window.innerHeight;
    const topLimit = cssPx("--safe-area-inset-top") + 2;
    const bottomLimit = vh - cssPx("--safe-area-inset-bottom") - 2;
    const leftLimit = cssPx("--safe-area-inset-left") + 4;
    const rightLimit = vw - cssPx("--safe-area-inset-right") - 4;
    const sx = Math.max(leftLimit, r.left - pad);
    const sy = Math.max(topLimit, r.top - pad);
    const sw = Math.min(r.right + pad, rightLimit) - sx;
    const sh = Math.min(r.bottom + pad, bottomLimit) - sy;
    spot.style.left = sx + "px";
    spot.style.top = sy + "px";
    spot.style.width = sw + "px";
    spot.style.height = sh + "px";

    clear(card);
    const nextBtn = h("button.btn.btn--seal", { style: "min-height:40px;padding:0 18px" },
      [idx === steps.length - 1 ? t("coach.done") : t("onb.next")]);
    nextBtn.addEventListener("click", () => { haptic("tap"); advance(); });
    const skipBtn = h("button.fa-btn", {}, [t("onb.skip")]);
    skipBtn.addEventListener("click", () => { haptic("tap"); finish(); });
    card.append(
      h("div.coach__title", {}, [step.title]),
      h("div.coach__body", {}, [step.body]),
      h("div.coach__row", {}, [skipBtn, h("div.coach__count", {}, [`${idx + 1}/${steps.length}`]), nextBtn]),
    );

    requestAnimationFrame(() => {
      const cw = card.offsetWidth, ch = card.offsetHeight;
      let top = sy + sh / 2 < vh / 2 ? sy + sh + 12 : sy - ch - 12;
      top = Math.max(topLimit + 6, Math.min(top, bottomLimit - ch - 6));
      let left = sx + sw / 2 - cw / 2;
      left = Math.max(leftLimit + 8, Math.min(left, rightLimit - cw - 8));
      card.style.top = top + "px";
      card.style.left = left + "px";
    });
  };
  void render();
}

// справка «Как это работает»

function sheetHowto() {
  const sections: [string, string][] = [
    ["howto.seedH", "howto.seedB"],
    ["howto.phraseH", "howto.phraseB"],
    ["howto.deriveH", "howto.deriveB"],
    ["howto.backupH", "howto.backupB"],
    ["howto.riskH", "howto.riskB"],
  ];
  openSheet(() => h("div.stack.gap-3", {}, [
    h("div.t-title", {}, [t("howto.title")]),
    ...sections.flatMap(([hk, bk]) => [
      h("div.t-section", { style: "margin-top:6px" }, [t(hk)]),
      h("div.t-body-2", { style: "line-height:1.6" }, [t(bk)]),
    ]),
  ]));
}

// встроенная справка «?»

function helpSheet(titleKey: string, bodyKey: string) {
  openSheet(() => h("div.stack.gap-3", {}, [
    h("div.t-title", {}, [t(titleKey)]),
    h("div.t-body-2", { style: "line-height:1.6" }, [t(bodyKey)]),
  ]));
}

function helpBtn(titleKey: string, bodyKey: string): HTMLElement {
  const b = h("button.help-btn", { "aria-label": "?" }, ["?"]);
  b.addEventListener("click", () => { haptic("tap"); helpSheet(titleKey, bodyKey); });
  return b;
}

function titleRow(text: string, titleKey?: string, bodyKey?: string): HTMLElement {
  const kids: (Node | string)[] = [h("div.t-title", {}, [text])];
  if (titleKey && bodyKey) kids.push(helpBtn(titleKey, bodyKey));
  return h("div", { style: "display:flex;align-items:center;justify-content:center;gap:8px" }, kids);
}

function setScreen(el: HTMLElement) {
  clear(app);
  app.append(el);
}

// переиспользуемые элементы

/** Свой переключатель вместо стандартного чекбокса. */
function switchToggle(labelText: string, checked = false): { el: HTMLElement; input: HTMLInputElement } {
  const input = h("input", { type: "checkbox" }) as HTMLInputElement;
  input.checked = checked;
  const el = h("label.switch", {}, [input, h("span.switch__track"), h("span.t-body-2", {}, [labelText])]);
  return { el, input };
}

type AnyField = HTMLInputElement | HTMLTextAreaElement;

/** Читаем буфер: сперва нативный плагин (в WebView надёжнее), потом navigator. */
async function pasteFromClipboard(): Promise<string> {
  try {
    const txt = await clipboardRead();
    if (txt) return txt;
  } catch { /* пробуем веб-путь ниже */ }
  try { return (await navigator.clipboard.readText()) || ""; } catch { return ""; }
}

/** Пишем в буфер нативно (на Android помечаем содержимое чувствительным),
    при осечке - через плагин, потом через navigator. */
async function copyToClipboard(text: string): Promise<void> {
  try { await clipCopy(text); return; } catch { /* пробуем плагин */ }
  try { await clipboardWrite(text); return; } catch { /* и веб-путь */ }
  await navigator.clipboard.writeText(text);
}

/** Очистка буфера: нативная, при осечке - пустой строкой. */
async function clearClipboard(): Promise<void> {
  try { await clipClear(); return; } catch { /* фолбэк */ }
  await copyToClipboard("");
}

function fieldTools(field: AnyField, opts: { reveal?: boolean; paste?: boolean; copy?: boolean }): HTMLElement {
  const btns: HTMLElement[] = [];
  if (opts.reveal && field instanceof HTMLInputElement) {
    const b = h("button.fa-btn", { type: "button" }, [icons.eye(), h("span", {}, [t("tools.show")])]);
    let shown = false;
    b.addEventListener("click", () => {
      shown = !shown;
      field.type = shown ? "text" : "password";
      clear(b);
      b.append(shown ? icons.eyeOff() : icons.eye(), h("span", {}, [shown ? t("tools.hide") : t("tools.show")]));
      haptic("tap");
    });
    btns.push(b);
  }
  if (opts.paste) {
    const b = h("button.fa-btn", { type: "button" }, [icons.paste(), h("span", {}, [t("tools.paste")])]);
    b.addEventListener("click", async () => {
      try {
        const txt = await pasteFromClipboard();
        if (txt) { field.value = txt; field.dispatchEvent(new Event("input", { bubbles: true })); haptic("tap"); }
        else toast(t("tools.clipEmpty"), "err");
      } catch { toast(t("tools.clipErr"), "err"); }
    });
    btns.push(b);
  }
  if (opts.copy) {
    const b = h("button.fa-btn", { type: "button" }, [icons.copy(), h("span", {}, [t("tools.copy")])]);
    b.addEventListener("click", async () => {
      try { await copyToClipboard(field.value); toast(t("tools.copied"), "ok"); haptic("confirm"); } catch {}
    });
    btns.push(b);
  }
  return h("div.field-tools", {}, btns);
}

/** Поле и панель действий под ним одним блоком. */
function withTools(field: AnyField, opts: { reveal?: boolean; paste?: boolean; copy?: boolean }): HTMLElement {
  return h("div.stack", { style: "gap:6px" }, [field, fieldTools(field, opts)]);
}

// автоблокировка

let unlocked = false;
let idleTimer = 0;

/** Таймаут бездействия в мс из настроек. 0 - «Никогда», автоблокировки нет. */
function getIdleMs(): number {
  try {
    const s = parseInt(localStorage.getItem("svitok.lockSec") || "120", 10);
    return s >= 0 ? s * 1000 : 120_000;
  } catch {
    return 120_000;
  }
}

/** Блокировать ли при уходе в фон. По умолчанию да. */
function lockOnBackground(): boolean {
  try { return localStorage.getItem("svitok.lockOnBg") !== "0"; } catch { return true; }
}

function lockNow() {
  if (!unlocked) return;
  unlocked = false;
  clearInterval(tickTimer);
  // закрываем все открытые листы
  while (sheetStack.length) sheetStack[sheetStack.length - 1]();
  api.lock().catch(() => {});
  clearClipboard().catch(() => {}); // вдруг в буфере остался пароль
  screenUnlock();
}

function resetIdle() {
  if (!unlocked) return;
  clearTimeout(idleTimer);
  const ms = getIdleMs();
  if (ms > 0) idleTimer = window.setTimeout(lockNow, ms);
}

// Блокировка при сворачивании, если включена в настройках. Во время
// создания и разблокировки unlocked=false, так что системный запрос
// биометрии сам блокировку не вызывает.
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "hidden" && lockOnBackground()) lockNow();
});

// Это приложение, а не веб-страница, поэтому глушим масштабирование жестами и Ctrl+колесом.
document.addEventListener("touchmove", (e) => {
  if ((e as TouchEvent).touches.length > 1) e.preventDefault();
}, { passive: false });
document.addEventListener("gesturestart", (e) => e.preventDefault());
document.addEventListener("wheel", (e) => {
  if ((e as WheelEvent).ctrlKey) e.preventDefault();
}, { passive: false });
document.addEventListener("keydown", (e) => {
  const ke = e as KeyboardEvent;
  if (ke.ctrlKey && (ke.key === "+" || ke.key === "-" || ke.key === "=" || ke.key === "0")) e.preventDefault();
});
for (const ev of ["pointerdown", "keydown"]) {
  document.addEventListener(ev, resetIdle, { passive: true });
}

// создание Свитка

function screenCreate() {
  unlocked = false;
  screenBack = null;
  clearTimeout(idleTimer);
  const newBtn = h("button.btn.btn--seal.btn--full", {}, [t("create.new")]);
  const restoreBtn = h("button.btn.btn--full", {}, [t("create.restore")]);
  newBtn.addEventListener("click", () => screenCreateNew());
  restoreBtn.addEventListener("click", () => screenRestore());
  setScreen(
    h("div.screen", {}, [
      h("div.grow.col-center.gap-4", { style: "justify-content:center;padding:0 20px" }, [
        logoScroll("logo--lg"),
        h("div.wordmark", {}, [t("app.name")]),
        h("div.t-body-2", { style: "text-align:center;margin:4px 0 16px" }, [t("app.tagline")]),
        newBtn, restoreBtn,
      ]),
    ])
  );
}

// Грубая прикидка стойкости фразы. Вся защита при краже листка держится на
// ней, поэтому откровенно слабые не пускаем.
type Strength = "empty" | "weak" | "ok" | "good";
function phraseStrength(p: string): Strength {
  const s = p.trim();
  if (!s) return "empty";
  const lc = s.toLowerCase();
  const words = s.split(/\s+/).filter(Boolean);
  if (/^(.)\1*$/.test(s)) return "weak"; // один символ подряд
  if (new Set(lc).size < 4) return "weak"; // почти нет разнообразия
  if (/^(0123|1234|12345|qwert|qwerty|passw|пароль|admin|abcd)/.test(lc)) return "weak";
  if (words.length >= 5 || s.length >= 16) return "good";
  if (words.length >= 4 || s.length >= 12) return "ok";
  return "weak";
}

function screenCreateNew() {
  screenBack = () => screenCreate();
  const p1 = h("input.field", { type: "password", placeholder: t("create.phrase"), autocomplete: "off" }) as HTMLInputElement;
  const p2 = h("input.field", { type: "password", placeholder: t("create.phraseRepeat"), autocomplete: "off" }) as HTMLInputElement;
  const hint = h("div.t-body-2.err", { style: "min-height:20px" });
  const meter = h("div.t-body-2", { style: "min-height:18px;text-align:center" });
  const create = h("button.btn.btn--seal.btn--full", {}, [t("create.submit")]);

  p1.addEventListener("input", () => {
    const lvl = phraseStrength(p1.value);
    if (lvl === "empty") { meter.textContent = ""; return; }
    const colors: Record<Strength, string> = { empty: "", weak: "var(--err)", ok: "var(--warn)", good: "var(--ok)" };
    meter.style.color = colors[lvl];
    meter.textContent = t("create.strength." + lvl);
  });

  create.addEventListener("click", async () => {
    hint.textContent = "";
    if (!p1.value) { hint.textContent = t("create.errShort"); return; }
    if (phraseStrength(p1.value) === "weak") { hint.textContent = t("create.weak"); return; }
    if (p1.value !== p2.value) { hint.textContent = t("create.errMismatch"); return; }
    create.setAttribute("disabled", "");
    create.textContent = t("create.computing");
    try {
      const res = await api.createVault(p1.value);
      haptic("confirm");
      confetti();
      screenSeedPaper(res.fingerprint, res.seedPaper);
    } catch (e) {
      hint.textContent = String(e);
      create.removeAttribute("disabled");
      create.textContent = t("create.submit");
    }
  });

  setScreen(
    h("div.screen", {}, [
      h("div.screen__center", {}, [
        titleRow(t("create.title"), "help.phrase.title", "help.phrase.body"),
        h("div.t-body-2", { style: "text-align:center" }, [t("create.hint")]),
        withTools(p1, { reveal: true }), meter, p2, hint, create,
      ]),
    ])
  );
}

function screenSeedPaper(fp: string, lines: string[]) {
  screenBack = () => screenMain();
  const done = h("button.btn.btn--seal.btn--full", {}, [t("seed.done")]);
  done.addEventListener("click", () => screenMain());
  setScreen(
    h("div.screen", {}, [
      h("div.screen__center", {}, [
        titleRow(t("seed.title"), "help.seed.title", "help.seed.body"),
        h("div.t-body-2.err", {}, [t("seed.warn")]),
        h("div.paper", {}, [
          h("div.t-body-2.faint", { style: "margin-bottom:10px" }, [t("seed.fp", { fp })]),
          ...lines.map(paperLine),
        ]),
        done,
      ]),
    ])
  );
}

function screenRestore() {
  screenBack = () => screenCreate();
  const seed = h("textarea.field.mono", {
    placeholder: t("restore.seedPh"),
    rows: "4",
    autocomplete: "off",
  }) as HTMLTextAreaElement;
  const phrase = h("input.field", { type: "password", placeholder: t("restore.phrasePh"), autocomplete: "off" }) as HTMLInputElement;
  const hint = h("div.t-body-2.err", { style: "min-height:20px" });
  const go = h("button.btn.btn--seal.btn--full", {}, [t("restore.submit")]);
  const back = h("button.btn.btn--ghost.btn--full", {}, [t("restore.back")]);
  back.addEventListener("click", () => screenCreate());

  go.addEventListener("click", async () => {
    hint.textContent = "";
    if (!seed.value.trim()) { hint.textContent = t("restore.errNoSeed"); return; }
    if (!phrase.value) { hint.textContent = t("restore.errNoPhrase"); return; }
    go.setAttribute("disabled", "");
    go.textContent = t("create.computing");
    try {
      const res = await api.restoreVault(phrase.value, seed.value);
      haptic("confirm");
      toast(t("restore.fpToast", { fp: res.fingerprint }), "ok");
      screenMain();
    } catch (e) {
      hint.textContent = String(e);
      go.removeAttribute("disabled");
      go.textContent = t("restore.submit");
    }
  });

  setScreen(
    h("div.screen", {}, [
      h("div.screen__center", {}, [
        h("div.t-title", { style: "text-align:center" }, [t("restore.title")]),
        h("div.t-body-2", {}, [t("restore.hint")]),
        withTools(seed, { paste: true }), withTools(phrase, { reveal: true }), hint, go, back,
      ]),
    ])
  );
}

// разблокировка

function screenUnlock() {
  mainScreenActive = false;
  unlocked = false;
  screenBack = null;
  clearTimeout(idleTimer);
  api.setScreenProtection(true).catch(() => {}); // на экране ввода фразы - всегда защищаем
  const logo = logoScroll("logo--lg");
  const bar = h("div.kdf-bar.hidden");
  const fpLine = h("div.t-body-2.faint", { style: "min-height:20px;text-align:center" });
  const phrase = h("input.field", { type: "password", placeholder: t("create.phrase"), autocomplete: "off" }) as HTMLInputElement;
  const open = h("button.btn.btn--seal.btn--full", {}, [t("unlock.open")]);
  const hint = h("div.t-body-2.err", { style: "min-height:20px;text-align:center" });
  let attempts = 0;

  async function tryUnlock() {
    if (!phrase.value) return;
    hint.textContent = "";
    open.setAttribute("disabled", "");
    bar.classList.remove("hidden");
    try {
      const res = await api.unlock(phrase.value);
      bar.classList.add("hidden");
      logo.classList.add("logo--pulse");
      fpLine.textContent = t("unlock.keyOk", { fp: res.fingerprint });
      haptic("confirm");
      setTimeout(() => screenMain(), 620);
    } catch (e) {
      bar.classList.add("hidden");
      open.removeAttribute("disabled");
      attempts++;
      logo.classList.remove("logo--shake");
      void logo.getBoundingClientRect(); // форсим reflow, чтобы анимация запустилась заново
      logo.classList.add("logo--shake");
      haptic("reject");
      hint.textContent = attempts >= 2 ? String(e) : t("unlock.wrong");
      phrase.select();
    }
  }
  open.addEventListener("click", tryUnlock);
  phrase.addEventListener("keydown", (e) => { if ((e as KeyboardEvent).key === "Enter") tryUnlock(); });

  setScreen(
    h("div.screen", {}, [
      h("div.grow.col-center.gap-4", { style: "justify-content:center" }, [
        logo,
        h("div.wordmark", {}, [t("app.name")]),
        bar, fpLine,
      ]),
      h("div.px.stack.gap-3.kb-pad", {}, [
        withTools(phrase, { reveal: true }), hint, open,
      ]),
    ])
  );
}

// главный экран с вкладками

type Tab = "sites" | "codes" | "vault";
let currentTab: Tab = "sites";
let tickTimer = 0;

function screenMain() {
  mainScreenActive = true;
  unlocked = true;
  screenBack = null;
  resetIdle();
  applyScreenProtectPref(); // разблокированы - применяем выбор пользователя
  const content = h("div.grow", { style: "position:relative;overflow:hidden" });
  if (IS_MOBILE) {
    const tabbar = h("div.tabbar", {}, [
      tabButton("sites", t("tab.sites"), icons.sites(), content),
      tabButton("codes", t("tab.codes"), icons.codes(), content),
      tabButton("vault", t("tab.vault"), icons.vault(), content),
    ]);
    setScreen(h("div.screen", {}, [content, tabbar]));
    selectTab(currentTab, content, tabbar);
    maybeCoach();
  } else {
    const nav = buildSidebar(content);
    shellContent = content;
    shellNav = nav;
    setScreen(h("div.screen.shell", {}, [nav, content]));
    selectTab(currentTab, content, nav);
  }
}

// Держим ссылки на панели десктопного шелла, чтобы «Настройки» рисовались
// внутри контента и сайдбар оставался, а не отдельным экраном с «назад».
let shellContent: HTMLElement | null = null;
let shellNav: HTMLElement | null = null;

/** Боковая навигация для десктопа: бренд, вкладки, а снизу настройки и блокировка. */
function buildSidebar(content: HTMLElement): HTMLElement {
  const nav = h("div.sidebar");
  const navItem = (tab: Tab, label: string, icon: SVGElement) => {
    const el = h("div.navitem", { "data-tab": tab }, [icon, h("span", {}, [label])]);
    el.addEventListener("click", () => selectTab(tab, content, nav));
    return el;
  };
  const action = (label: string, icon: SVGElement, onClick: () => void, nk?: string) => {
    const el = h("div.navitem.navitem--action", nk ? { "data-nav": nk } : {}, [icon, h("span", {}, [label])]);
    el.addEventListener("click", onClick);
    return el;
  };
  nav.append(
    h("div.nav__brand", {}, [logoScroll("logo--sm"), h("div.wordmark", {}, [t("app.name")])]),
    navItem("sites", t("tab.sites"), icons.sites()),
    navItem("codes", t("tab.codes"), icons.codes()),
    navItem("vault", t("tab.vault"), icons.vault()),
    h("div.grow"),
    action(t("settings.title"), icons.gear(), () => screenSettings(), "settings"),
    action(t("vault.lock"), icons.lock(), () => lockNow()),
  );
  return nav;
}

function tabButton(tab: Tab, label: string, icon: SVGElement, content: HTMLElement): HTMLElement {
  const btn = h("div.tab", { "data-tab": tab }, [icon, h("span", {}, [label])]);
  btn.addEventListener("click", () => {
    haptic("tap");
    selectTab(tab, content, btn.parentElement as HTMLElement);
  });
  return btn;
}

function selectTab(tab: Tab, content: HTMLElement, tabbar: HTMLElement) {
  currentTab = tab;
  clearInterval(tickTimer);
  for (const el of Array.from(tabbar.children)) {
    el.classList.toggle("tab--active", el.getAttribute("data-tab") === tab);
  }
  if (tab === "sites") renderSites(content);
  else if (tab === "codes") renderCodes(content);
  else renderVault(content);
}

// вкладка: Сайты

async function renderSites(content: HTMLElement) {
  clear(content);
  const search = h("input.field", { placeholder: t("sites.search"), autocomplete: "off" }) as HTMLInputElement;
  const list = h("div.stack");
  const scroll = h("div.screen__scroll", {}, [h("div.px", { style: "padding-bottom:12px" }, [search]), list]);
  const fab = h("button.fab", { "aria-label": t("sites.addAria") }, ["+"]);
  fab.addEventListener("click", () => { haptic("tap"); sheetAddSite(() => renderSites(content)); });
  content.append(h("div.stack", { style: "height:100%" }, [
    h("div.screen__head", {}, [h("div.t-title", {}, [t("tab.sites")])]),
    scroll,
  ]), fab);

  let sites: SiteView[] = [];
  try { sites = await api.listSites(); } catch (e) { toast(String(e), "err"); }
  const draw = (filter: string) => {
    clear(list);
    const q = filter.trim().toLowerCase();
    const shown = sites.filter((s) => !q || s.name.toLowerCase().includes(q) || s.login.toLowerCase().includes(q));
    if (!shown.length) {
      if (sites.length) {
        list.append(h("div.empty", {}, [t("sites.notFound")]));
      } else {
        const add = h("button.btn.btn--seal", { style: "margin-top:16px" }, [t("sites.add")]);
        add.addEventListener("click", () => { haptic("tap"); sheetAddSite(() => renderSites(content)); });
        list.append(
          h("div.empty", {}, [t("sites.emptyDesc")]),
          h("div.center", {}, [add]),
        );
      }
      return;
    }
    shown.forEach((s, i) => list.append(siteRow(s, i + 1, content)));
  };
  search.addEventListener("input", () => draw(search.value));
  draw("");
}

function siteRow(s: SiteView, num: number, content: HTMLElement): HTMLElement {
  const sub = s.login ? s.login : t("sites.noLogin");
  const row = h("div.row.tap", {}, [
    h("div.row__num", {}, [String(num).padStart(2, "0")]),
    h("div.row__main", {}, [
      h("div.row__name", {}, [s.name]),
      h("div.row__sub", {}, [sub]),
    ]),
    h("div.row__side", {}, [s.counter > 1 ? h("span.faint", {}, ["v" + s.counter]) : h("span")]),
    icons.chev(),
  ]);
  row.addEventListener("click", () => { haptic("tap"); sheetPassword(s, () => renderSites(content)); });
  return row;
}

// вкладка: Коды (TOTP)

interface CodeState { label: string; code: string; digits: number; left: number; period: number; el: HTMLElement; }

async function renderCodes(content: HTMLElement) {
  clear(content);
  const list = h("div.stack");
  const fab = h("button.fab", { "aria-label": t("codes.addAria") }, ["+"]);
  fab.addEventListener("click", () => { haptic("tap"); sheetAddTotp(() => renderCodes(content)); });
  content.append(h("div.stack", { style: "height:100%" }, [
    h("div.screen__head", {}, [h("div.t-title", {}, [t("tab.codes")])]),
    h("div.screen__scroll", {}, [list]),
  ]), fab);

  let labels: string[] = [];
  try { labels = await api.totpList(); } catch (e) { toast(String(e), "err"); }
  if (!labels.length) {
    list.append(h("div.empty", {}, [t("codes.empty")]));
    return;
  }
  const states: CodeState[] = [];
  for (const label of labels) {
    try {
      const tc = await api.totpCode(label);
      const st: CodeState = { label, code: tc.code, digits: tc.digits, left: tc.secondsLeft, period: tc.period, el: h("div") };
      st.el = totpRow(st);
      states.push(st);
      list.append(st.el);
    } catch { /* битую запись просто пропускаем */ }
  }
  clearInterval(tickTimer);
  tickTimer = window.setInterval(() => tick(states), 1000);
}

let totpClipTimer = 0;

function totpRow(st: CodeState): HTMLElement {
  const codeEl = h("div.t-totp", {}, [groupSecret(st.code, 3)]);
  const ring = svgEl(
    '<circle class="ring__bg" cx="12" cy="12" r="10" fill="none" stroke-width="2"/>' +
      '<circle class="ring__fg" cx="12" cy="12" r="10" fill="none" stroke-width="2" stroke-linecap="round"/>',
    "ring"
  );
  const row = h("div.row.tap", { style: "min-height:72px" }, [
    h("div.row__main", {}, [
      h("div.row__sub", {}, [st.label]),
      codeEl,
    ]),
    h("div.row__side", {}, [ring as unknown as Node]),
  ]);
  (st as CodeState & { codeEl: HTMLElement; ring: SVGElement }).codeEl = codeEl;
  (st as CodeState & { codeEl: HTMLElement; ring: SVGElement }).ring = ring;
  updateRing(st, ring);
  row.addEventListener("click", async () => {
    await copyToClipboard(st.code).catch(() => {});
    haptic("tap");
    toast(t("codes.copied"), "ok");
    // код живёт недолго, но в буфере он не должен оставаться дольше окна - чистим,
    // как и пароли
    clearTimeout(totpClipTimer);
    totpClipTimer = window.setTimeout(() => { void clearClipboard().catch(() => {}); }, 30000);
  });
  return row;
}

function updateRing(st: CodeState, ring: SVGElement) {
  const fg = ring.querySelector(".ring__fg") as SVGCircleElement;
  const circ = 2 * Math.PI * 10;
  const frac = st.left / st.period;
  fg.style.strokeDasharray = String(circ);
  fg.style.strokeDashoffset = String(circ * (1 - frac));
  ring.classList.toggle("ring--warn", st.left <= 5);
}

async function tick(states: CodeState[]) {
  for (const st of states) {
    st.left--;
    const s = st as CodeState & { codeEl: HTMLElement; ring: SVGElement };
    if (st.left <= 0) {
      try {
        const tc = await api.totpCode(st.label);
        st.code = tc.code; st.left = tc.secondsLeft;
        s.codeEl.textContent = groupSecret(st.code, 3);
      } catch { st.left = st.period; }
    }
    updateRing(st, s.ring);
  }
}

// вкладка: Сейф

async function renderVault(content: HTMLElement) {
  clear(content);
  const list = h("div.stack");
  const banner = h("div.stack");
  if (isBackupStale()) {
    const b = h("div.backup-banner", {}, [t("backup.remind")]);
    b.addEventListener("click", () => { haptic("tap"); sheetBackup(); });
    banner.append(b);
  }
  const actions = h("div.px.stack.gap-2", { style: "padding-top:8px" }, [
    vaultAddBtn(t("vault.addPw"), () => sheetAddSecret("password", () => renderVault(content)), icons.key()),
    vaultAddBtn(t("vault.addTotp"), () => sheetAddTotp(() => renderVault(content)), icons.shield()),
    vaultAddBtn(t("vault.addCodes"), () => sheetAddSecret("codes", () => renderVault(content)), icons.ticket()),
    vaultAddBtn(t("vault.addNote"), () => sheetAddSecret("note", () => renderVault(content)), icons.note()),
  ]);
  content.append(h("div.stack", { style: "height:100%" }, [
    h("div.screen__head", {}, [h("div.t-title", {}, [t("tab.vault")])]),
    h("div.screen__scroll", {}, [
      banner,
      list,
      h("div.t-section.px", { style: "margin:24px 0 8px" }, [t("vault.addSection")]),
      actions,
      h("div.t-section.px", { style: "margin:24px 0 8px" }, [t("vault.other")]),
      h("div.px.stack.gap-2", {}, [
        vaultAddBtn(t("vault.showPaper"), () => sheetPaper(), icons.doc()),
        // На десктопе Настройки и Блокировка живут в сайдбаре, поэтому тут они только на телефоне.
        ...(IS_MOBILE ? [
          (() => {
            const b = vaultAddBtn(t("vault.settings"), () => screenSettings(), icons.gear());
            b.setAttribute("data-coach", "settings");
            return b;
          })(),
          vaultAddBtn(t("vault.lock"), () => lockNow(), icons.lock()),
        ] : []),
      ]),
    ]),
  ]));

  let entries: EntryView[] = [];
  try { entries = await api.vaultList(); } catch (e) { toast(String(e), "err"); }
  if (!entries.length) {
    list.append(h("div.empty", {}, [t("vault.empty")]));
    return;
  }
  entries.forEach((e, i) => {
    const row = h("div.row.tap", {}, [
      h("div.row__num", {}, [String(i + 1).padStart(2, "0")]),
      h("div.row__main", {}, [
        h("div.row__name", {}, [e.label]),
        h("div.row__sub.faint", {}, [t("kind." + e.kind)]),
      ]),
    ]);
    row.addEventListener("click", () => { haptic("long"); sheetEntry(e, () => renderVault(content)); });
    list.append(row);
  });
}

function vaultAddBtn(label: string, onClick: () => void, icon?: SVGElement): HTMLElement {
  const kids: (Node | string)[] = icon ? [icon, label] : [label];
  const b = h("button.btn.btn--full", { style: "justify-content:flex-start;gap:10px" }, kids);
  b.addEventListener("click", () => { haptic("tap"); onClick(); });
  return b;
}

// нижние листы (bottom sheets)

// Стек открытых листов нужен для аппаратной кнопки «назад» на Android.
const sheetStack: (() => void)[] = [];

function openSheet(build: (close: () => void) => HTMLElement) {
  const desktop = !IS_MOBILE;
  const scrim = h("div.scrim");
  const sheet = h(desktop ? "div.modal" : "div.sheet");
  let closed = false;
  const close = () => {
    if (closed) return;
    closed = true;
    const i = sheetStack.indexOf(close);
    if (i >= 0) sheetStack.splice(i, 1);
    scrim.classList.remove("scrim--open");
    if (desktop) {
      sheet.classList.remove("modal--open");
    } else {
      sheet.style.transition = "";
      sheet.style.transform = "translateY(100%)";
    }
    setTimeout(() => { scrim.remove(); sheet.remove(); }, 320);
  };
  sheetStack.push(close);

  // На десктопе это центрированный диалог, без ползунка и свайпа.
  if (desktop) {
    sheet.append(build(close));
    scrim.addEventListener("click", close);
    document.body.append(scrim, sheet);
    requestAnimationFrame(() => {
      scrim.classList.add("scrim--open");
      sheet.classList.add("modal--open");
    });
    return;
  }

  // На телефоне нижний лист с ползунком: свайп вниз закрывает.
  const handle = h("div.sheet__grab", {}, [h("div.sheet__handle")]);
  let startY = 0, curY = 0, dragging = false;
  handle.addEventListener("pointerdown", (e) => {
    const pe = e as PointerEvent;
    dragging = true; startY = pe.clientY; curY = 0;
    sheet.style.transition = "none";
    handle.setPointerCapture(pe.pointerId);
  });
  handle.addEventListener("pointermove", (e) => {
    if (!dragging) return;
    curY = Math.max(0, (e as PointerEvent).clientY - startY);
    sheet.style.transform = `translateY(${curY}px)`;
  });
  const endDrag = () => {
    if (!dragging) return;
    dragging = false;
    sheet.style.transition = "";
    if (curY > 110) { haptic("tap"); close(); }
    else sheet.style.transform = "translateY(0)";
  };
  handle.addEventListener("pointerup", endDrag);
  handle.addEventListener("pointercancel", endDrag);

  sheet.append(handle, build(close));
  scrim.addEventListener("click", close);
  document.body.append(scrim, sheet);
  requestAnimationFrame(() => {
    scrim.classList.add("scrim--open");
    sheet.classList.add("sheet--open");
    sheet.style.transform = "translateY(0)";
  });
}

// Обработчик системной кнопки «назад», его дёргает MainActivity.kt.
interface AndroidBridge { exit(): void; }
declare global {
  interface Window {
    __svitokBack?: () => void;
    __svitokAndroid?: AndroidBridge;
  }
}
window.__svitokBack = () => {
  // 1) открыт умный тур - закрываем его
  const coach = document.querySelector(".coach");
  if (coach) { try { localStorage.setItem("svitok.coached", "1"); } catch {} coach.remove(); return; }
  // 2) открыт нижний лист - закрываем верхний
  if (sheetStack.length) { sheetStack[sheetStack.length - 1](); return; }
  // 3) полноэкранный подэкран (Настройки, Восстановление) - у него свой обработчик
  if (screenBack) { screenBack(); return; }
  // 4) не на вкладке «Сайты» - возвращаемся на неё
  if (isMainScreen() && currentTab !== "sites") {
    const content = document.querySelector("#app .screen > .grow") as HTMLElement | null;
    const nav = document.querySelector("#app .tabbar, #app .sidebar") as HTMLElement | null;
    if (content && nav) { selectTab("sites", content, nav); return; }
  }
  // 5) корень: на телефоне выходим, на десктопе ничего не делаем
  window.__svitokAndroid?.exit();
};

// На десктопе Esc закрывает тур, лист или подэкран (как «назад»), но в корне
// из приложения не выходит - только те же шаги 1-4.
if (!IS_MOBILE) {
  document.addEventListener("keydown", (e) => {
    if ((e as KeyboardEvent).key !== "Escape") return;
    if (document.querySelector(".coach")) return window.__svitokBack?.();
    if (sheetStack.length) { sheetStack[sheetStack.length - 1](); return; }
    if (screenBack) { screenBack(); return; }
  });
}

let mainScreenActive = false;
let screenBack: (() => void) | null = null;
function isMainScreen() { return mainScreenActive; }

function sheetPassword(s: SiteView, refresh: () => void) {
  openSheet((close) => {
    let clearTimer = 0;
    const dots = "•".repeat(Math.min(s.length, 20));
    const value = h("div.t-secret.selectable", { style: "word-break:break-all;min-height:56px" }, [dots]);
    let pw = "";

    const ensure = async (): Promise<string> => {
      if (!pw) pw = (await api.derivePassword(s.name)).password;
      return pw;
    };
    let held = false;
    const reveal = async (on: boolean) => {
      held = on;
      if (!on) { value.textContent = dots; return; }
      const shown = groupSecret(await ensure(), 4);
      // за время await деривации кнопку могли уже отпустить - тогда не показываем,
      // иначе пароль залипнет на экране после отпускания
      if (held) value.textContent = shown;
    };
    const holdBtn = h("button.btn.btn--full", {}, [t("pw.reveal")]);
    const start = async (e: Event) => { e.preventDefault(); await reveal(true); };
    const end = () => reveal(false);
    holdBtn.addEventListener("pointerdown", start);
    holdBtn.addEventListener("pointerup", end);
    holdBtn.addEventListener("pointerleave", end);
    holdBtn.addEventListener("pointercancel", end);

    const copyBtn = h("button.btn.btn--seal.btn--full", {}, [t("pw.copy")]);
    copyBtn.addEventListener("click", async () => {
      const v = await ensure();
      await copyToClipboard(v).catch(() => {});
      haptic("confirm");
      let left = 30;
      copyBtn.textContent = t("pw.clearing", { n: left });
      clearInterval(clearTimer);
      clearTimer = window.setInterval(async () => {
        left--;
        if (left <= 0) {
          clearInterval(clearTimer);
          await clearClipboard().catch(() => {});
          copyBtn.textContent = t("pw.copy");
        } else copyBtn.textContent = t("pw.clearing", { n: left });
      }, 1000);
    });

    const bigBtn = h("button.btn.btn--full", {}, [t("pw.big")]);
    bigBtn.addEventListener("click", async () => sheetLargeType(await ensure()));
    const qrBtn = h("button.btn.btn--full", {}, [t("pw.qr")]);
    qrBtn.addEventListener("click", async () => sheetQr(await ensure()));
    const bumpBtn = h("button.btn.btn--ghost.btn--full", {}, [icons.bump(), t("pw.bump", { n: s.counter })]);
    bumpBtn.addEventListener("click", async () => {
      try { const c = await api.bumpSite(s.name); markBackupStale(); toast(t("pw.bumped", { n: c }), "ok"); close(); refresh(); }
      catch (e) { toast(String(e), "err"); }
    });
    const editBtn = h("button.btn.btn--ghost.btn--full", {}, [icons.edit(), t("pw.edit")]);
    editBtn.addEventListener("click", () => { close(); sheetAddSite(refresh, s); });

    void reveal(false);
    return h("div.stack.gap-3", {}, [
      h("div.t-title", {}, [s.name]),
      h("div.t-body-2", {}, [s.login ? t("pw.login", { l: s.login }) : t("pw.noLogin"), "  ·  " + t("pw.length", { n: s.length })]),
      value,
      copyBtn, holdBtn,
      h("div", { style: "display:flex;gap:8px" }, [bigBtn, qrBtn]),
      h("div", { style: "display:flex;gap:8px" }, [bumpBtn, editBtn]),
    ]);
  });
}

function sheetLargeType(pw: string) {
  openSheet(() => h("div.stack.gap-4", {}, [
    h("div.t-section", {}, [t("large.title")]),
    h("div.t-large.selectable", { style: "word-break:break-all" }, [groupSecret(pw, 4)]),
  ]));
}

function sheetQr(data: string) {
  openSheet(() => {
    const holder = h("div.center", {
      style: "background:#F5F0E8;border-radius:16px;padding:8px;max-width:320px;margin:0 auto;width:100%",
    });
    api.qrSvg(data)
      .then((svg) => {
        holder.innerHTML = svg;
        const el = holder.querySelector("svg");
        if (el) { el.style.width = "100%"; el.style.height = "auto"; el.style.display = "block"; }
      })
      .catch((e) => { holder.textContent = String(e); });
    return h("div.stack.gap-3", {}, [
      h("div.t-title", {}, [t("qr.title")]),
      h("div.t-body-2", {}, [t("qr.hint")]),
      holder,
    ]);
  });
}

function sheetAddSite(refresh: () => void, edit?: SiteView) {
  openSheet((close) => {
    const isEdit = !!edit;
    const name = h("input.field", { placeholder: t("addsite.namePh"), autocomplete: "off", value: edit?.name ?? "" }) as HTMLInputElement;
    if (isEdit) { name.readOnly = true; name.style.opacity = "0.6"; }
    const login = h("input.field", { placeholder: t("addsite.loginPh"), autocomplete: "off", value: edit?.login ?? "" }) as HTMLInputElement;
    const len = h("input.field.mono", { type: "number", value: String(edit?.length ?? 20), inputmode: "numeric" }) as HTMLInputElement;
    const src = edit?.classes ?? "luds";
    const cls = { l: src.includes("l"), u: src.includes("u"), d: src.includes("d"), s: src.includes("s") };
    const chips = h("div", { style: "display:flex;gap:8px;flex-wrap:wrap" },
      ([["l", "abc"], ["u", "ABC"], ["d", "123"], ["s", "#@!"]] as const).map(([k, lbl]) => {
        const chip = h("button.btn", { style: "min-height:40px;padding:0 14px" }, [lbl]);
        const paint = () => {
          chip.style.background = cls[k] ? "var(--seal)" : "var(--surface-2)";
          chip.style.color = cls[k] ? "var(--on-seal)" : "var(--text)";
        };
        paint();
        chip.addEventListener("click", () => { cls[k] = !cls[k]; paint(); });
        return chip;
      }));
    const err = h("div.t-body-2.err", { style: "min-height:20px" });
    const save = h("button.btn.btn--seal.btn--full", {}, [isEdit ? t("addsite.save") : t("addsite.add")]);
    save.addEventListener("click", async () => {
      const classes = (["l", "u", "d", "s"] as const).filter((k) => cls[k]).join("");
      if (!name.value.trim()) { err.textContent = t("addsite.errName"); return; }
      if (!classes) { err.textContent = t("addsite.errClass"); return; }
      try {
        if (isEdit) await api.updateSite(edit!.name, login.value.trim(), edit!.counter, Number(len.value) || 20, classes, null);
        else await api.addSite(name.value.trim(), login.value.trim(), 1, Number(len.value) || 20, classes, null);
        markBackupStale(); haptic("confirm"); close(); refresh();
      } catch (e) { err.textContent = String(e); }
    });

    const children: (Node | string)[] = [
      h("div.t-title", {}, [isEdit ? t("addsite.editTitle") : t("addsite.title")]),
      name, login,
      h("div.t-body-2", {}, [t("addsite.lenLabel")]), len,
      h("div.t-body-2", {}, [t("addsite.charsLabel")]), chips,
    ];
    if (isEdit) {
      children.push(h("div.t-body-2.faint", { style: "line-height:1.5" }, [t("addsite.editNote")]), err, save);
      // удаляем с инлайн-подтверждением, без блокирующего confirm()
      const delWrap = h("div.stack.gap-2", { style: "margin-top:6px" });
      const delBtn = h("button.btn.btn--danger.btn--full", {}, [icons.del(), t("addsite.delete")]);
      delBtn.addEventListener("click", () => {
        clear(delWrap);
        const yes = h("button.btn.btn--danger.btn--full", {}, [t("addsite.deleteConfirm")]);
        yes.addEventListener("click", async () => {
          try { await api.removeSite(edit!.name); markBackupStale(); haptic("confirm"); close(); refresh(); }
          catch (e) { err.textContent = String(e); }
        });
        const no = h("button.btn.btn--ghost.btn--full", {}, [t("common.cancel")]);
        no.addEventListener("click", () => { clear(delWrap); delWrap.append(delBtn); });
        delWrap.append(h("div.t-body-2", {}, [t("addsite.deleteAsk", { name: edit!.name })]), yes, no);
      });
      delWrap.append(delBtn);
      children.push(delWrap);
    } else {
      children.push(err, save);
    }
    return h("div.stack.gap-3", {}, children);
  });
}

function sheetAddTotp(refresh: () => void) {
  openSheet((close) => {
    const label = h("input.field", { placeholder: t("addtotp.labelPh"), autocomplete: "off" }) as HTMLInputElement;
    const secret = h("input.field.mono", { placeholder: t("addtotp.secretPh"), autocomplete: "off" }) as HTMLInputElement;
    const d8sw = switchToggle(t("addtotp.digits8"));
    const d8 = d8sw.input;
    const err = h("div.t-body-2.err", { style: "min-height:20px" });
    const save = h("button.btn.btn--seal.btn--full", {}, [t("addtotp.add")]);

    // период из отсканированного otpauth (30 по умолчанию для ручного ввода)
    let period = 30;

    // Сканируем otpauth-QR с сайта (только на телефоне).
    const scanBtn = h("button.btn.btn--full", {}, [icons.camera(), t("addtotp.scan")]);
    scanBtn.addEventListener("click", async () => {
      err.textContent = "";
      haptic("tap");
      try {
        const content = await scanQr();
        if (!content) return;
        const otp = parseOtpauth(content);
        if (!otp) { err.textContent = t("scan.notOtp"); return; }
        label.value = otp.label;
        secret.value = otp.secret;
        d8.checked = otp.digits8;
        period = otp.period;
        haptic("confirm");
      } catch { err.textContent = t("scan.noCamera"); }
    });
    save.addEventListener("click", async () => {
      if (!label.value.trim() || !secret.value.trim()) { err.textContent = t("addtotp.errFill"); return; }
      try {
        const tc = await api.vaultAddTotp(label.value.trim(), secret.value.replace(/\s/g, ""), d8.checked, period);
        markBackupStale(); haptic("confirm");
        toast(t("addtotp.added", { code: tc.code }), "ok");
        close(); refresh();
      } catch (e) { err.textContent = String(e); }
    });
    return h("div.stack.gap-3", {}, [
      h("div.t-title", {}, [t("addtotp.title")]),
      ...(IS_MOBILE ? [scanBtn] : []),
      label,
      withTools(secret, { paste: true }),
      d8sw.el,
      err, save,
    ]);
  });
}

function sheetAddSecret(kind: "password" | "note" | "codes", refresh: () => void) {
  const titleKey = { password: "secret.pwTitle", note: "secret.noteTitle", codes: "secret.codesTitle" }[kind];
  openSheet((close) => {
    const label = h("input.field", { placeholder: t("secret.labelPh"), autocomplete: "off" }) as HTMLInputElement;
    const bodyPh = kind === "codes" ? t("secret.codesPh") : kind === "note" ? t("secret.notePh") : t("secret.pwPh");
    const body = (kind === "note" || kind === "codes"
      ? h("textarea.field", { placeholder: bodyPh, rows: "5" })
      : h("input.field", { placeholder: bodyPh, autocomplete: "off" })) as HTMLInputElement | HTMLTextAreaElement;
    const err = h("div.t-body-2.err", { style: "min-height:20px" });
    const save = h("button.btn.btn--seal.btn--full", {}, [t("secret.save")]);
    save.addEventListener("click", async () => {
      if (!label.value.trim() || !body.value.trim()) { err.textContent = t("secret.errFill"); return; }
      try {
        if (kind === "password") await api.vaultAddPassword(label.value.trim(), body.value);
        else if (kind === "note") await api.vaultAddNote(label.value.trim(), body.value);
        else await api.vaultAddCodes(label.value.trim(), body.value.split("\n").map((s) => s.trim()).filter(Boolean));
        markBackupStale(); haptic("confirm"); close(); refresh();
      } catch (e) { err.textContent = String(e); }
    });
    const bodyOpts = kind === "password" ? { reveal: true, paste: true } : { paste: true };
    return h("div.stack.gap-3", {}, [h("div.t-title", {}, [t(titleKey)]), label, withTools(body, bodyOpts), err, save]);
  });
}

function sheetEntry(e: EntryView, refresh: () => void) {
  openSheet((close) => {
    const del = h("button.btn.btn--full", { style: "color:var(--err)" }, [t("entry.delete")]);
    del.addEventListener("click", async () => {
      try { await api.vaultRemove(e.label); markBackupStale(); haptic("confirm"); close(); refresh(); }
      catch (err) { toast(String(err), "err"); }
    });
    return h("div.stack.gap-3", {}, [
      h("div.t-title", {}, [e.label]),
      h("div.t-body-2.faint", {}, [t("entry.type", { k: t("kind." + e.kind) })]),
      del,
    ]);
  });
}

async function sheetPaper() {
  let paper;
  try { paper = await api.paperExport(); } catch (e) { toast(String(e), "err"); return; }
  openSheet(() => h("div.stack.gap-3", {}, [
    h("div.t-title", {}, [t("paper.title")]),
    h("div.t-section", {}, ["KDF " + paper.kdf]),
    h("div.t-section", { style: "margin-top:8px" }, [t("tab.sites")]),
    ...paper.sites.map((l) => h("div.mono.t-body-2.selectable", {}, [l])),
    ...(paper.vault.length
      ? [h("div.t-section", { style: "margin-top:8px" }, [t("tab.vault")]), h("div.paper", {}, paper.vault.map(paperLine))]
      : []),
  ]));
}

/** Показать сид ещё раз, чтобы переписать на новый листок. Требуем повторить
 * фразу: без этого молчаливый вызов из JS мог бы выгрузить сид. На Android к
 * тому же чтение сида идёт под биометрией. */
function sheetShowSeed() {
  openSheet(() => {
    const box = h("div.paper");
    const err = h("div.t-body-2.err", { style: "min-height:20px" });
    const phrase = h("input.field", { type: "password", placeholder: t("showseed.phrasePh"), autocomplete: "off" }) as HTMLInputElement;
    const phraseWrap = withTools(phrase, { reveal: true });
    const go = h("button.btn.btn--seal.btn--full", {}, [t("showseed.reveal")]);
    let shown = false;
    go.addEventListener("click", async () => {
      if (shown) return;
      if (!phrase.value) { err.textContent = t("showseed.needPhrase"); return; }
      err.textContent = "";
      go.textContent = t("showseed.revealing");
      try {
        const lines = await api.showSeed(phrase.value);
        phrase.value = "";
        shown = true;
        lines.forEach((l) => box.append(paperLine(l)));
        phraseWrap.remove();
        go.remove();
        haptic("confirm");
      } catch (e) { err.textContent = String(e); go.textContent = t("showseed.reveal"); }
    });
    return h("div.stack.gap-3", {}, [
      h("div.t-title", {}, [t("showseed.title")]),
      h("div.t-body-2", { style: "line-height:1.6" }, [t("showseed.hint")]),
      phraseWrap,
      go,
      box,
      err,
    ]);
  });
}

boot();
