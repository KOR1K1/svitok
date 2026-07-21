// Типизированные обёртки над IPC. Один к одному с командами из src-tauri/commands.rs.
// Секреты (сид, мастер-ключ) сюда не долетают - только готовые результаты.
import { invoke } from "@tauri-apps/api/core";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";

/** Читаем буфер нативно: на Android WebView navigator.clipboard не отдаёт. */
export async function clipboardRead(): Promise<string> {
  return (await readText()) ?? "";
}
export async function clipboardWrite(text: string): Promise<void> {
  await writeText(text);
}
/** Нативная копия: на Android помечается «чувствительной», на десктопе обычная. */
export const clipCopy = (text: string) => invoke<void>("clip_copy", { text });
export const clipClear = () => invoke<void>("clip_clear");

export interface Status { hasVault: boolean; hasSeed: boolean; unlocked: boolean; }
export interface NewVault { fingerprint: string; seedPaper: string[]; }
export interface Unlocked { fingerprint: string; }
export interface SiteView { name: string; login: string; counter: number; length: number; classes: string; }
export interface PasswordView { name: string; login: string; counter: number; password: string; }
export interface EntryView { kind: string; label: string; }
export interface TotpView { label: string; code: string; digits: number; secondsLeft: number; period: number; }
export interface Paper { kdf: string; sites: string[]; vault: string[]; }

export const api = {
  status: () => invoke<Status>("status"),
  createVault: (phrase: string) => invoke<NewVault>("create_vault", { phrase }),
  restoreVault: (phrase: string, seed: string) => invoke<Unlocked>("restore_vault", { phrase, seed }),
  unlock: (phrase: string) => invoke<Unlocked>("unlock", { phrase }),
  lock: () => invoke<void>("lock"),
  destroyVault: () => invoke<void>("destroy_vault"),

  listSites: () => invoke<SiteView[]>("list_sites"),
  addSite: (name: string, login: string, counter: number, length: number, classes: string, symbols: string | null) =>
    invoke<void>("add_site", { name, login, counter, length, classes, symbols }),
  bumpSite: (name: string) => invoke<number>("bump_site", { name }),
  updateSite: (name: string, login: string, counter: number, length: number, classes: string, symbols: string | null) =>
    invoke<void>("update_site", { name, login, counter, length, classes, symbols }),
  removeSite: (name: string) => invoke<void>("remove_site", { name }),
  showSeed: () => invoke<string[]>("show_seed"),
  derivePassword: (name: string) => invoke<PasswordView>("derive_password", { name }),

  vaultList: () => invoke<EntryView[]>("vault_list"),
  totpList: () => invoke<string[]>("totp_list"),
  totpCode: (label: string) => invoke<TotpView>("totp_code", { label }),
  vaultAddTotp: (label: string, secretB32: string, digits8: boolean, period: number) =>
    invoke<TotpView>("vault_add_totp", { label, secretB32, digits8, period }),
  vaultAddPassword: (label: string, secret: string) => invoke<void>("vault_add_password", { label, secret }),
  vaultAddNote: (label: string, text: string) => invoke<void>("vault_add_note", { label, text }),
  vaultAddCodes: (label: string, codes: string[]) => invoke<void>("vault_add_codes", { label, codes }),
  vaultRemove: (label: string) => invoke<void>("vault_remove", { label }),

  qrSvg: (data: string) => invoke<string>("qr_svg", { data }),
  setScreenProtection: (on: boolean) => invoke<void>("set_screen_protection", { on }),
  backupExport: () => invoke<string>("backup_export"),
  backupImport: (data: string) => invoke<number>("backup_import", { data }),
  syncExport: () => invoke<string>("sync_export"),
  syncImport: (data: string) => invoke<number>("sync_import", { data }),
  paperExport: () => invoke<Paper>("paper_export"),
};
