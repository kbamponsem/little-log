import type { Baby, Caregiver, Entry, JoinProgress, JoinRequest, Role, SyncStatus, Theme } from "./model";
import type { StorageAdapter } from "./storage";

/**
 * The app's data API. Components talk to the repository, never straight to the
 * adapter, so caching or a second storage backend stays local.
 */
export class Repository {
  constructor(private readonly adapter: StorageAdapter) {}

  /** This device's id — its public key. There is no login to perform. */
  identity(): Promise<string> {
    return this.adapter.identity();
  }

  /** This device's appearance preference. */
  theme(): Promise<Theme> {
    return this.adapter.theme();
  }

  saveTheme(theme: Theme): Promise<void> {
    return this.adapter.saveTheme(theme);
  }

  /* entries */

  listEntries(): Promise<Entry[]> {
    return this.adapter.listEntries();
  }

  saveEntry(entry: Entry): Promise<Entry> {
    return this.adapter.saveEntry(entry);
  }

  deleteEntry(entryId: string): Promise<void> {
    return this.adapter.deleteEntry(entryId);
  }

  /* children + caregivers */

  babies(): Promise<Baby[]> {
    return this.adapter.babies();
  }

  currentBaby(): Promise<Baby | null> {
    return this.adapter.currentBaby();
  }

  createBaby(name: string, born: string): Promise<Baby> {
    return this.adapter.createBaby(name, born);
  }

  saveBaby(baby: Baby): Promise<Baby> {
    return this.adapter.saveBaby(baby);
  }

  setCurrentBaby(babyId: string): Promise<void> {
    return this.adapter.setCurrentBaby(babyId);
  }

  caregivers(): Promise<Caregiver[]> {
    return this.adapter.caregivers();
  }

  invite(role: Role): Promise<string> {
    return this.adapter.invite(role);
  }

  requestJoin(link: string): Promise<void> {
    return this.adapter.requestJoin(link);
  }

  pollJoin(link: string): Promise<JoinProgress> {
    return this.adapter.pollJoin(link);
  }

  joinRequests(): Promise<JoinRequest[]> {
    return this.adapter.joinRequests();
  }

  resolveJoin(device: string, accept: boolean): Promise<void> {
    return this.adapter.resolveJoin(device, accept);
  }

  syncStatus(): Promise<SyncStatus> {
    return this.adapter.syncStatus();
  }

  forceSync(): Promise<SyncStatus> {
    return this.adapter.forceSync();
  }

  /* data */

  csv(): Promise<string> {
    return this.adapter.csv();
  }

  deleteAll(): Promise<void> {
    return this.adapter.deleteAll();
  }

  takeWidgetAction(): Promise<string | null> {
    return this.adapter.takeWidgetAction();
  }

  savePhoto(entryId: string, blob: Blob): Promise<string> {
    return this.adapter.savePhoto(entryId, blob);
  }

  fillPhotos(root: HTMLElement): void | Promise<void> {
    return this.adapter.fillPhotos(root);
  }

  saveBabyPhoto(babyId: string, blob: Blob): Promise<string> {
    return this.adapter.saveBabyPhoto(babyId, blob);
  }

  babyPhotoDataUrl(babyId: string): Promise<string> {
    return this.adapter.babyPhotoDataUrl(babyId);
  }
}
