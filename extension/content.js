// Находит поля логина/пароля. По фокусу спрашивает у приложения совпадения
// (op:"match" - лёгкий пик, работает и на заблокированном ваулте). По клику
// шлёт op:"fill": если Свиток заблокирован, приложение всплывёт и подождёт
// ввода фразы, после чего тем же запросом вернёт пароль - и поля заполнятся
// без повторных действий.
(() => {
  const origin = location.origin;
  let dropdown = null;
  let lastField = null;

  function isVisible(el) {
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0 && el.offsetParent !== null;
  }

  function passwordInputs() {
    return [...document.querySelectorAll('input[type="password"]')].filter(isVisible);
  }

  function usernameInputs() {
    const sel = 'input[type="text"], input[type="email"], input:not([type])';
    return [...document.querySelectorAll(sel)].filter((el) => {
      if (!isVisible(el)) return false;
      const ac = (el.autocomplete || "").toLowerCase();
      const name = ((el.name || "") + " " + (el.id || "")).toLowerCase();
      return (
        ac.includes("username") ||
        ac.includes("email") ||
        el.type === "email" ||
        /user|email|login|phone|tel/.test(name)
      );
    });
  }

  function setValue(el, val) {
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
    setter.call(el, val);
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  }

  function fillFields(m) {
    if (m.login) {
      const user = usernameInputs()[0];
      if (user) setValue(user, m.login);
    }
    if (m.password) {
      for (const p of passwordInputs()) setValue(p, m.password);
    }
  }

  function removeDropdown() {
    if (dropdown) {
      dropdown.remove();
      dropdown = null;
    }
  }

  function baseBox(field) {
    removeDropdown();
    const r = field.getBoundingClientRect();
    dropdown = document.createElement("div");
    dropdown.className = "svitok-af";
    dropdown.style.left = window.scrollX + r.left + "px";
    dropdown.style.top = "-9999px"; // до замера высоты
    dropdown.style.minWidth = Math.max(r.width, 240) + "px";
    dropdown._rect = r;
    return dropdown;
  }

  // Рисуем над полем, если сверху есть место: гугловская выпадашка всегда снизу,
  // так мы с ней не пересекаемся. Места нет - падаем под поле.
  function place(box) {
    document.body.appendChild(box);
    const r = box._rect;
    const hgt = box.offsetHeight;
    box.style.top =
      (r.top > hgt + 8 ? window.scrollY + r.top - hgt - 4 : window.scrollY + r.bottom + 4) + "px";
  }

  function note(field, text) {
    const box = baseBox(field);
    const n = document.createElement("div");
    n.className = "svitok-af__note";
    n.textContent = text;
    box.appendChild(n);
    place(box);
  }

  function showMatches(field, items, locked) {
    const box = baseBox(field);
    for (const m of items) {
      const row = document.createElement("div");
      row.className = "svitok-af__row";
      const title = document.createElement("div");
      title.className = "svitok-af__name";
      title.textContent = "Свиток · " + m.name;
      row.appendChild(title);
      if (m.login) {
        const sub = document.createElement("div");
        sub.className = "svitok-af__login";
        sub.textContent = m.login;
        row.appendChild(sub);
      }
      row.addEventListener("mousedown", (e) => {
        e.preventDefault();
        choose(field, m, locked);
      });
      box.appendChild(row);
    }
    if (locked) {
      const n = document.createElement("div");
      n.className = "svitok-af__note";
      n.textContent = "Свиток заблокирован - откроется при выборе";
      box.appendChild(n);
    }
    place(box);
  }

  async function choose(field, m, locked) {
    note(field, locked ? "Разблокируйте Свиток…" : "Заполняю…");
    let resp;
    try {
      resp = await chrome.runtime.sendMessage({ op: "fill", origin, name: m.name });
    } catch {
      removeDropdown();
      return;
    }
    if (resp && resp.ok && resp.matches && resp.matches.length) {
      fillFields(resp.matches[0]);
      removeDropdown();
    } else {
      note(field, "Свиток: " + (resp && resp.error ? resp.error : "не вышло"));
      setTimeout(removeDropdown, 2500);
    }
  }

  async function onFocus(field) {
    lastField = field;
    let resp;
    try {
      resp = await chrome.runtime.sendMessage({ op: "match", origin });
    } catch {
      return; // старый content script после reload - молчим
    }
    if (field !== lastField || !resp || !resp.ok) return;
    if (resp.matches && resp.matches.length) {
      showMatches(field, resp.matches, !!resp.locked);
    }
  }

  document.addEventListener("focusin", (e) => {
    const el = e.target;
    if (!el || !el.matches || !el.matches("input")) return;
    if (el.type === "password" || usernameInputs().includes(el)) {
      onFocus(el);
    }
  });
  document.addEventListener("mousedown", (e) => {
    if (dropdown && !dropdown.contains(e.target)) removeDropdown();
  });
  window.addEventListener("scroll", removeDropdown, true);
})();
