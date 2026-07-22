// Service worker: единственный, кто говорит с native-messaging хостом Свитка.
// Content script присылает сюда запрос на заполнение, мы дёргаем хост (он
// пересылает в приложение) и возвращаем результат. Токен связки лежит в
// chrome.storage, его вводят один раз в настройках расширения.

const HOST = "app.svitok.host";

function callHost(message) {
  return new Promise((resolve) => {
    try {
      chrome.runtime.sendNativeMessage(HOST, message, (resp) => {
        if (chrome.runtime.lastError) {
          resolve({ ok: false, error: "host-missing" });
        } else {
          resolve(resp || { ok: false, error: "empty" });
        }
      });
    } catch (e) {
      resolve({ ok: false, error: "host-missing" });
    }
  });
}

chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (msg && msg.op === "fill") {
    chrome.storage.local.get("token").then(({ token }) => {
      callHost({ op: "fill", origin: msg.origin, token: token || "" }).then(sendResponse);
    });
    return true; // ответ асинхронный
  }
  return false;
});
