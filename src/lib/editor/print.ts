/**
 * Printing: pages are rendered to print-resolution JPEGs, shown in a hidden
 * print-only container, and handed to the system print dialog (printer,
 * copies, "print to file" are all handled there). On Linux the dialog is
 * opened natively with the project's paper size, because WebKitGTK ignores
 * CSS @page sizes; elsewhere window.print() and @page do the job.
 */
import { listen } from '@tauri-apps/api/event';
import { api, fileUrl, isTauri } from '../api/backend';
import type { CollageDoc } from '../model/types';
import { app } from '../stores/app.svelte';
import { exportPages, joinPath } from './exporter';

const ROOT_ID = 'tessera-print-root';
const STYLE_ID = 'tessera-print-style';

function cleanup(): void {
  document.getElementById(ROOT_ID)?.remove();
  document.getElementById(STYLE_ID)?.remove();
}

/** Page size in millimetres. */
export function pageMillimetres(doc: CollageDoc): [number, number] {
  return [(doc.page.width / doc.page.dpi) * 25.4, (doc.page.height / doc.page.dpi) * 25.4];
}

export async function printPages(
  doc: CollageDoc,
  pages: number[],
  onProgress?: (fraction: number, stage: string) => void,
): Promise<void> {
  cleanup();
  const dir = await api.ensureDir(joinPath(app.info?.cacheDir ?? '/tmp', 'print'));
  // 300 DPI is plenty for any printer; larger pages are scaled down.
  const scale = Math.min(1, 300 / doc.page.dpi);
  const results = await exportPages(doc, pages, {
    path: joinPath(dir, 'tessera-print.jpg'),
    format: 'jpeg',
    quality: 93,
    scale,
    unique: false,
    onProgress,
  });
  const [wmm, hmm] = pageMillimetres(doc);
  const style = document.createElement('style');
  style.id = STYLE_ID;
  style.textContent = `
    #${ROOT_ID} { display: none; }
    @page { size: ${wmm.toFixed(2)}mm ${hmm.toFixed(2)}mm; margin: 0; }
    @media print {
      html, body { height: auto !important; overflow: visible !important; background: #fff !important; }
      body > *:not(#${ROOT_ID}) { display: none !important; }
      #${ROOT_ID} { display: block !important; }
      #${ROOT_ID} img { display: block; width: ${wmm.toFixed(2)}mm; height: ${hmm.toFixed(2)}mm; break-after: page; page-break-after: always; }
      #${ROOT_ID} img:last-child { break-after: auto; page-break-after: auto; }
    }`;
  const root = document.createElement('div');
  root.id = ROOT_ID;
  const stamp = Date.now();
  const imgs = results.map((r) => {
    const img = document.createElement('img');
    img.src = `${fileUrl(r.path)}?v=${stamp}`;
    img.alt = '';
    root.appendChild(img);
    return img;
  });
  document.head.appendChild(style);
  document.body.appendChild(root);
  await Promise.all(imgs.map((i) => i.decode().catch(() => undefined)));
  onProgress?.(1, 'Printing');

  if (!isTauri) {
    window.addEventListener('afterprint', cleanup, { once: true });
    window.print();
    return;
  }
  // The job renders the page content after the dialog closes, so the print
  // container stays until WebKit reports the job finished or failed. The
  // listeners go up before the dialog opens so a quick job can't be missed.
  let settle: (err?: string) => void = () => undefined;
  const finished = new Promise<void>((resolve, reject) => {
    settle = (err) => (err ? reject(new Error(err)) : resolve());
  });
  const unlisten = await Promise.all([
    listen('print-finished', () => settle()),
    listen<string>('print-failed', (e) => settle(e.payload || 'Printing failed')),
  ]);
  const timer = setTimeout(() => settle(), 10 * 60_000);
  try {
    const result = await api.printNative(wmm, hmm, doc.name || 'Tessera');
    if (result === 'unsupported') {
      window.addEventListener('afterprint', cleanup, { once: true });
      window.print();
      return;
    }
    if (result === 'printing') await finished;
    cleanup();
  } catch (e) {
    cleanup();
    throw e;
  } finally {
    clearTimeout(timer);
    for (const off of unlisten) off();
  }
}
