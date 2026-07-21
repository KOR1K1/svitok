// Скан QR камерой, только на мобильных. Плагин barcode-scanner грузим
// динамически - на десктопе он в бандл не попадёт.
// С камеры ничего наружу не уходит: приложение работает без сети.

export const IS_MOBILE = /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);

/** Открываем камеру и отдаём первый QR. Нет доступа - кидаем "no-camera". */
export async function scanQr(): Promise<string | null> {
  const bs = await import("@tauri-apps/plugin-barcode-scanner");
  let state = await bs.checkPermissions();
  if (state !== "granted") state = await bs.requestPermissions();
  if (state !== "granted") throw new Error("no-camera");
  const res = await bs.scan({ windowed: false, formats: [] });
  return res.content ?? null;
}

export interface Otp { label: string; secret: string; digits8: boolean; period: number; }

/** Разбираем otpauth://totp/… - такой QR даёт аутентификатор сайта. */
export function parseOtpauth(uri: string): Otp | null {
  if (!/^otpauth:\/\/totp\//i.test(uri)) return null;
  try {
    const u = new URL(uri);
    const secret = (u.searchParams.get("secret") || "").replace(/\s/g, "");
    if (!secret) return null;
    const digits = parseInt(u.searchParams.get("digits") || "6", 10);
    const period = parseInt(u.searchParams.get("period") || "30", 10);
    let label = decodeURIComponent(u.pathname.replace(/^\/+/, ""));
    const issuer = u.searchParams.get("issuer");
    if (!label && issuer) label = issuer;
    // period держим в разумных рамках, метку не даём разрастись
    const safePeriod = period >= 15 && period <= 120 ? period : 30;
    return { label: (label || "totp").slice(0, 64), secret, digits8: digits === 8, period: safePeriod };
  } catch {
    return null;
  }
}
