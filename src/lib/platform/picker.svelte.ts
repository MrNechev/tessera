/** State of the in-app server file picker (web mode). See ServerPicker.svelte. */

export interface PickerRequest {
  mode: 'files' | 'folder' | 'save';
  title: string;
  startPath?: string;
  /** Lower-case extensions to show (files/save); empty = all files. */
  extensions?: string[];
  multiple?: boolean;
  /** Only folders the server may write to can be chosen. */
  writableOnly?: boolean;
  /** Save mode: suggested file name. */
  fileName?: string;
}

class PickerState {
  current = $state<PickerRequest | null>(null);
  /** Browser mode: download results after saving them on the server. */
  downloadResults = $state(true);
  /** Remembered between openings. */
  lastFolder = $state<string | null>(null);
  private resolve: ((v: string[] | null) => void) | null = null;

  open(req: PickerRequest): Promise<string[] | null> {
    this.finish(null);
    this.current = req;
    return new Promise((r) => (this.resolve = r));
  }

  finish(result: string[] | null): void {
    const r = this.resolve;
    this.resolve = null;
    this.current = null;
    r?.(result);
  }
}

export const picker = new PickerState();
