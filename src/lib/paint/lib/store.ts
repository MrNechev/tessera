/**
 * Paint documents on the engine: layered `.ora` files in the app's project
 * library (`projects/drawings/`, next to the collage projects) and the
 * record of which exported image came from which drawing.
 */
import { call, callBinary } from '../../api/http';

export interface DrawingSummary {
  id: string;
  name: string;
  /** RFC 3339. */
  modified: string;
  width: number;
  height: number;
  /** Absolute path of the .ora on the engine's machine. */
  path: string;
  /** Absolute path of the thumbnail PNG (serve with fileUrl). */
  thumb?: string | null;
}

export const drawings = {
  list: () => call<DrawingSummary[]>('list_drawings'),
  save: (id: string, name: string, ora: Uint8Array) =>
    callBinary<DrawingSummary>(`/api/t/save_drawing?id=${encodeURIComponent(id)}&name=${encodeURIComponent(name)}`, ora),
  async load(id: string): Promise<Uint8Array> {
    const res = await fetch(`/api/t/drawing?id=${encodeURIComponent(id)}`, { credentials: 'same-origin', cache: 'no-store' });
    if (!res.ok) throw new Error(res.status === 404 ? 'This drawing no longer exists' : `Could not open the drawing (${res.status})`);
    return new Uint8Array(await res.arrayBuffer());
  },
  rename: (id: string, name: string) => call<DrawingSummary>('rename_drawing', { id, name }),
  delete: (id: string) => call<void>('delete_drawing', { id }),
  /** Remember that the image at `path` (engine machine) was exported from drawing `id`. */
  linkExport: (id: string, path: string) => call<void>('link_drawing_export', { id, path }),
  /** The drawing an exported image came from, if it still exists. */
  forExport: (path: string) => call<string | null>('drawing_for_export', { path }),
};
