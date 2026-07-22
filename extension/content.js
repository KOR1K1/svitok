// Находит поля логина/пароля, при фокусе спрашивает у приложения совпадения для
// текущего origin и показывает выпадашку. Клик подставляет логин и пароль.
// Пароль приходит уже выведенным из приложения - расширение его только вставляет.
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

  function fillMatch(m) {
    if (m.login) {
      const user = usernameInputs()[0];
      if (user) setValue(user, m.login);
    }
    for (const p of passwordInputs()) setValue(p, m.password);
    removeDropdown();
  }

  function removeDropdown() {
    if (dropdown) {
      dropdown.remove();
      dropdown = null;
    }
  }

  function showDropdown(field, items, note) {
    removeDropdown();
    const r = field.getBoundingClientRect();
    dropdown = document.createElement("div");
    dropdown.className = "svitok-af";
    dropdown.style.left = window.scrollX + r.left + "px";
    dropdown.style.top = window.scrollY + r.bottom + 4 + "px";
    dropdown.style.minWidth = Math.max(r.width, 220) + "px";
    if (note) {
      const n = document.createElement("div");
      n.className = "svitok-af__note";
      n.textContent = note;
      dropdown.appendChild(n);
    }
    for (const m of items) {
      const row = document.createElement("div");
      row.className = "svitok-af__row";
      row.textContent = "Свиток · " + m.name + (m.login ? " (" + m.login + ")" : "");
      row.addEventListener("mousedown", (e) => {
        e.preventDefault();
        fillMatch(m);
      });
      dropdown.appendChild(row);
    }
    document.body.appendChild(dropdown);
  }

  async function onFocus(field) {
    lastField = field;
    let resp;
    try {
      resp = await chrome.runtime.sendMessage({ op: "fill", origin });
    } catch {
      return;
    }
    if (field !== lastField || !resp) return;
    if (resp.ok && resp.matches && resp.matches.length) {
      showDropdown(field, resp.matches, null);
    } else if (!resp.ok && resp.error === "locked") {
      showDropdown(field, [], "Разблокируйте Свиток");
    }
    // host-missing / unpaired / no match - молчим, чтобы не мешать
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
