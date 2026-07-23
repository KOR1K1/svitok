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
export interface SiteView { id: string; name: string; login: string; counter: number; length: number; classes: string; aliases: string[]; label: string; }
export interface PasswordView { name: string; login: string; counter: number; password: string; }
export interface EntryView { kind: string; label: string; }
export interface TotpView { label: string; code: string; digits: number; secondsLeft: number; period: number; }
export interface Paper { kdf: string; sites: string[]; vault: string[]; }
export interface SyncPreview { added: string[]; updated: string[]; }

export const api = {
  status: () => invoke<Status>("status"),
  createVault: (phrase: string) => invoke<NewVault>("create_vault", { phrase }),
  restoreVault: (phrase: string, seed: string) => invoke<Unlocked>("restore_vault", { phrase, seed }),
  unlock: (phrase: string) => invoke<Unlocked>("unlock", { phrase }),
  lock: () => invoke<void>("lock"),
  destroyVault: () => invoke<void>("destroy_vault"),

  listSites: () => invoke<SiteView[]>("list_sites"),
  // add/update возвращают мягкие предупреждения о пересечении доменов с другими записями
  addSite: (name: string, login: string, counter: number, length: number, classes: string, symbols: string | null, aliases: string[], label: string) =>
    invoke<string[]>("add_site", { name, login, counter, length, classes, symbols, aliases, label }),
  bumpSite: (id: string) => invoke<number>("bump_site", { id }),
  updateSite: (id: string, login: string, counter: number, length: number, classes: string, symbols: string | null, aliases: string[], label: string) =>
    invoke<string[]>("update_site", { id, login, counter, length, classes, symbols, aliases, label }),
  removeSite: (id: string) => invoke<void>("remove_site", { id }),
  showSeed: (phrase: string) => invoke<string[]>("show_seed", { phrase }),
  derivePassword: (id: string) => invoke<PasswordView>("derive_password", { id }),

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
  syncPreview: (data: string) => invoke<SyncPreview>("sync_preview", { data }),
  syncImport: (data: string, overwrite: boolean) => invoke<number>("sync_import", { data, overwrite }),
  autofillToken: () => invoke<string>("autofill_token"),
  paperExport: () => invoke<Paper>("paper_export"),
};
