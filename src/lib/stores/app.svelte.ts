/** Global application state: active view, settings, app info, updates. */
import { api, setImportExtensions } from '../api/backend';
import { picker } from '../platform/picker.svelte';
import type { AppInfo, ImportedImage, Settings, UpdateInfo } from '../api/types';
import { stickers } from './stickers.svelte';
import { toasts } from './toasts.svelte';

export type View = 'home' | 'library' | 'documents' | 'editor' | 'paint' | 'convert' | 'compress' | 'batch' | 'settings' | 'system';

export function defaultSettings(): Settings {
  return {
    saveDirectory: '',
    filenamePattern: '{name} {date} {time}',
    exportFormat: 'jpeg',
    exportQuality: 92,
    theme: 'system',
    checkUpdates: true,
    lastUpdateCheck: null,
    defaultPreset: 'a4',
    defaultDpi: 300,
    defaultOrientation: 'landscape',
    recentImportDir: null,
    showWelcome: true,
    downloadResults: true,
    libraries: [],
    setupDone: false,
  };
}

class AppState {
  view = $state<View>('home');
  info = $state<AppInfo | null>(null);
  settings = $state<Settings>(defaultSettings());
  update = $state<UpdateInfo | null>(null);
  ready = $state(false);
  /** Number of open modal dialogs; editor shortcuts stay quiet while one is up. */
  modals = $state(0);
  /** The "new photo book / calendar" dialog (opened from Home or the command palette). */
  newProject = $state<'book' | 'calendar' | null>(null);
  /** Photos to start that project with (chosen in the Library). */
  newProjectPhotos = $state<ImportedImage[] | null>(null);
  /** The template gallery on the Home screen. */
  templateGallery = $state(false);

  async init(): Promise<void> {
    void stickers.load();
    try {
      const [info, settings] = await Promise.all([api.appInfo(), api.loadSettings()]);
      this.info = info;
      setImportExtensions(info.inputExtensions);
      this.settings = { ...defaultSettings(), ...settings };
      picker.downloadResults = this.settings.downloadResults ?? true;
    } catch (e) {
      toasts.push({ kind: 'error', title: 'Could not load settings', message: String(e) });
    }
    this.applyTheme();
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => this.applyTheme());
    this.ready = true;
    void this.autoCheckUpdates();
  }

  async saveSettings(patch: Partial<Settings>): Promise<void> {
    this.settings = { ...this.settings, ...patch };
    this.applyTheme();
    picker.downloadResults = this.settings.downloadResults ?? true;
    try {
      await api.saveSettings($state.snapshot(this.settings) as Settings);
    } catch (e) {
      toasts.push({ kind: 'error', title: 'Could not save settings', message: String(e) });
    }
  }

  applyTheme(): void {
    const pref = this.settings.theme;
    const dark = pref === 'dark' || (pref === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
    document.documentElement.dataset.theme = dark ? 'dark' : 'light';
  }

  /** Running as the self-hosted web app (Docker), not the desktop app. */
  get isServer(): boolean {
    return this.info?.mode === 'server';
  }

  get isFlatpak(): boolean {
    return this.info?.installKind === 'flatpak';
  }

  async checkUpdates(manual = false): Promise<UpdateInfo | null> {
    if (this.isServer && !manual) return null;
    try {
      const info = await api.checkUpdates();
      this.update = info;
      await this.saveSettings({ lastUpdateCheck: new Date().toISOString() });
      if (info.hasUpdate) {
        toasts.push({
          kind: 'info',
          title: `Tessera ${info.latest} is available`,
          message: info.installKind === 'flatpak' ? 'Update it from your software centre.' : 'A new version is ready to download.',
          action: info.url ? { label: 'View release', run: () => void api.openUrl(info.url!) } : undefined,
          timeout: 0,
        });
      } else if (manual) {
        toasts.push({ kind: 'success', title: 'You’re up to date', message: `Tessera ${info.current} is the latest version.` });
      }
      return info;
    } catch (e) {
      if (manual) toasts.push({ kind: 'error', title: 'Update check failed', message: String(e) });
      return null;
    }
  }

  private async autoCheckUpdates(): Promise<void> {
    if (!this.settings.checkUpdates || this.isFlatpak || this.info?.installKind === 'dev') return;
    const last = this.settings.lastUpdateCheck ? Date.parse(this.settings.lastUpdateCheck) : 0;
    if (Date.now() - last < 24 * 3600 * 1000) return;
    setTimeout(() => void this.checkUpdates(false), 4000);
  }
}

export const app = new AppState();
