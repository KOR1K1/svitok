// Скан QR камерой, только на мобильных. Сканер свой (ScannerActivity:
// CameraX + ZXing), без Google ML Kit - и с камеры ничего наружу не уходит:
// приложение работает без сети.

import { invoke } from "@tauri-apps/api/core";
import { t } from "./i18n";

export const IS_MOBILE = /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);

// сканер - отдельная активность, главное окно на это время уходит в фон;
// блокировка «при сворачивании» не должна принимать его за сворачивание
let scanning = false;
export const isScanning = () => scanning;

/** Открываем камеру и отдаём первый QR. Отмена - null, нет доступа - "no-camera". */
export async function scanQr(): Promise<string | null> {
  scanning = true;
  try {
    return await invoke<string>("scan_qr", { hint: t("scan.hint") });
  } catch (e) {
    const msg = String(e);
    if (msg.includes("cancel")) return null;
    if (msg.includes("no-camera")) throw new Error("no-camera");
    throw e;
  } finally {
    scanning = false;
  }
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
