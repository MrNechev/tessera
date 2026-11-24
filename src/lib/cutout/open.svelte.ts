/**
 * Opening the shared "Cut out" dialog from anywhere (Library, collage,
 * stories). Deliberately tiny: the dialog, the worker and onnxruntime are
 * only loaded once someone actually asks for a cut-out.
 */

/** The engine's library id of Outputs (tessera_core::tools::OUTPUTS_ID). */
export const OUTPUTS_LIBRARY = 'outputs';
/** Where cut-outs are saved, inside Outputs. */
export const CUTOUTS_DIR = 'Cut-outs';

export interface CutoutResult {
  /** Absolute path of the saved PNG on the engine's machine. */
  path: string;
  /** The same file as a library reference (documents use these). */
  ref: { libraryId: string; path: string };
  width: number;
  height: number;
}

export interface CutoutAction {
  id: string;
  label: string;
}

export interface CutoutRequest {
  /** The photo: an absolute path on the engine's machine, or a library reference. */
  source: { path: string; name: string } | { libraryId: string; path: string; name: string };
  /** What to do with the saved cut-out (one button each); default: just save. */
  actions?: CutoutAction[];
  onsaved?: (r: CutoutResult, action: string) => void;
}

class CutoutDialogState {
  request = $state<CutoutRequest | null>(null);
  /** Bumped after every save (the Stickers panel's "My cut-outs" reloads). */
  saved = $state(0);
}

export const cutoutDialog = new CutoutDialogState();

export function openCutout(req: CutoutRequest): void {
  cutoutDialog.request = req;
}
