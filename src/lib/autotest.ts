/**
 * End-to-end smoke-test driver. Inactive unless the backend reports a script
 * via the TESSERA_AUTOTEST environment variable; then it replays a small,
 * fixed set of high-level steps (no arbitrary code) and logs to stdout.
 */
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { api } from './api/backend';
import type { ExportFormat } from './api/types';
import { editor } from './editor/editor.svelte';
import { exportDocument, exportPages } from './editor/exporter';
import { buildPhotoBook, buildPhotoCalendar } from './editor/projects';
import { newBlank, newFromTemplate, newQuickCollage, openEditor, printProject } from './editor/session.svelte';
import { instantiateTemplate } from './templates/instantiate';
import { createCalendar, createSticker } from './model/factory';
import type { LayoutEngine, LayoutShape, PhotoElement, TextElement } from './model/types';
import { app, type View } from './stores/app.svelte';
import { compressQueue, convertQueue, toolsQueue } from './stores/batch.svelte';
import { library } from './stores/library.svelte';
import { stickers } from './stores/stickers.svelte';
import { templateById } from './templates/catalog';
import { defaultMetadataPolicy } from './api/types';

type Step = Record<string, unknown> & { do: string };

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const log = (msg: string) => invoke('autotest_log', { msg }).catch(() => {});

async function run(step: Step): Promise<void> {
  const s = step as Record<string, any>; // eslint-disable-line @typescript-eslint/no-explicit-any
  switch (step.do) {
    case 'wait':
      await sleep(Number(s.ms ?? 500));
      break;
    case 'view':
      app.view = s.view as View;
      break;
    case 'quick':
      await newQuickCollage(s.photos as string[]);
      break;
    case 'blank':
      newBlank();
      break;
    case 'import':
      await library.import(s.photos as string[]);
      break;
    case 'template': {
      const t = templateById(s.id as string);
      if (!t) throw new Error(`no template ${s.id}`);
      if (s.photos) {
        // Fill the template with exactly these photos, in this order.
        const imgs = await library.import(s.photos as string[]);
        const doc = instantiateTemplate(t, {
          photos: imgs.map((i) => ({ id: i.id, ref: { path: i.path, name: i.name, width: i.width, height: i.height } })),
          stickerAspect: (id) => stickers.aspect(id),
        });
        openEditor(doc);
      } else newFromTemplate(t, true);
      break;
    }
    case 'addFree': {
      const imgs = await library.import(s.photos as string[]);
      editor.addFreePhotos(imgs.map((i) => i.id));
      break;
    }
    case 'engine':
      editor.setEngine(s.engine as LayoutEngine);
      if (s.shape) editor.setLayout({ shape: s.shape as LayoutShape });
      break;
    case 'settings':
      await app.saveSettings(s.settings as never);
      break;
    case 'pageSize':
      editor.setPage(Number(s.width), Number(s.height), Number(s.dpi ?? 300), s.preset as string | undefined);
      break;
    case 'deselect':
      editor.clearSelection();
      break;
    case 'tool':
      editor.tool = s.tool as never;
      break;
    case 'regenerate':
      editor.regenerate();
      break;
    case 'layout':
      editor.setLayout(s.settings as never);
      break;
    case 'background':
      editor.update((d) => {
        Object.assign(d.background, s.background as object);
      });
      break;
    case 'frame':
      editor.update((d) => {
        d.frame = s.frame as never;
      });
      break;
    case 'freePhotos': {
      const ids = library.items.slice(Number(s.from ?? 0), Number(s.to ?? 99)).map((i) => i.id);
      editor.addFreePhotos(ids);
      break;
    }
    case 'patchPhotos': {
      // Apply a patch to free photos, one per entry (position, rotation, frame…).
      const patches = s.patches as Partial<PhotoElement>[];
      const photos = editor.doc.elements.filter((e) => e.kind === 'photo');
      patches.forEach((p, i) => photos[i] && editor.patch(photos[i].id, p));
      break;
    }
    case 'text': {
      const el = editor.addText(s.text as Partial<TextElement>, s.at as { x: number; y: number } | undefined);
      void el;
      break;
    }
    case 'sticker': {
      await stickers.load();
      const S = editor.pageShort * Number(s.size ?? 0.15);
      const aspect = stickers.aspect(s.id as string);
      const w = aspect >= 1 ? S : S * aspect;
      const h = aspect >= 1 ? S / aspect : S;
      const W = editor.doc.page.width;
      const H = editor.doc.page.height;
      editor.place(createSticker({ sticker: s.id as string, x: 0, y: 0, w, h, rotation: Number(s.rotation ?? 0) }), {
        x: Number(s.x ?? 0.5) * W,
        y: Number(s.y ?? 0.5) * H,
      });
      break;
    }
    case 'calendar': {
      const S = editor.pageShort;
      const el = createCalendar({ x: 0, y: 0, w: S * Number(s.w ?? 0.6), h: S * Number(s.h ?? 0.5), ...(s.props as object) });
      const W = editor.doc.page.width;
      const H = editor.doc.page.height;
      editor.place(el, { x: Number(s.x ?? 0.5) * W, y: Number(s.y ?? 0.5) * H });
      break;
    }
    case 'select': {
      const kind = s.kind as string;
      const el = editor.doc.elements.filter((e) => e.kind === kind)[Number(s.index ?? 0)];
      if (el) editor.select([el.id]);
      break;
    }
    case 'swap': {
      const photos = editor.doc.elements.filter((e) => e.kind === 'photo');
      editor.swapPhotos(photos[Number(s.a)].id, photos[Number(s.b)].id);
      break;
    }
    case 'rename':
      editor.rename(s.name as string);
      break;
    case 'export': {
      const res = await exportDocument(editor.doc, {
        path: s.path as string,
        format: (s.format as ExportFormat) ?? 'jpeg',
        quality: Number(s.quality ?? 90),
        scale: Number(s.scale ?? 1),
      });
      await log(`exported ${res.path} ${res.width}x${res.height} ${res.bytes}B`);
      break;
    }
    case 'convert': {
      await convertQueue.add(s.photos as string[]);
      const res = await api.convertImages(
        convertQueue.files.map((f) => f.path),
        {
          format: { kind: 'to', format: s.format as never },
          quality: s.reduce ? { kind: 'reduce', percent: Number(s.reduce) } : { kind: 'original' },
          resize: { kind: 'none' },
          metadata: { ...defaultMetadataPolicy(), ...(s.metadata as object) },
          output: { directory: s.out as string, suffix: '', conflict: 'rename' },
          background: [255, 255, 255],
          color: (s.color as never) ?? 'srgb',
        },
      );
      convertQueue.results = res;
      await log(`convert ${JSON.stringify(res.map((r) => [r.status, r.output, r.inputBytes, r.outputBytes, r.message]))}`);
      break;
    }
    case 'compress': {
      await compressQueue.add(s.photos as string[]);
      const res = await api.compressImages(
        compressQueue.files.map((f) => f.path),
        {
          level: { kind: (s.level as never) ?? 'visuallyLossless' } as never,
          format: { kind: 'keep' },
          resize: { kind: 'none' },
          metadata: defaultMetadataPolicy(),
          output: { directory: s.out as string, suffix: '-compressed', conflict: 'rename' },
          keepIfLarger: true,
          color: 'keep',
        },
      );
      compressQueue.results = res;
      await log(`compress ${JSON.stringify(res.map((r) => [r.status, r.quality, r.ssim, r.inputBytes, r.outputBytes]))}`);
      break;
    }
    case 'batchQueue':
      await toolsQueue.add(s.photos as string[]);
      break;
    case 'batch': {
      const res = await api.batchProcess(s.jobs as never, s.options as never);
      toolsQueue.results = res;
      await log(`batch ${JSON.stringify(res.map((r) => [r.status, r.output, r.width, r.height, r.message]))}`);
      break;
    }
    case 'photobook': {
      const imgs = await library.import(s.photos as string[]);
      openEditor(buildPhotoBook(imgs, { presetId: 'scrapbook-12', orientation: 'landscape', perPage: 0, title: 'Our Story', subtitle: '2026', style: 'clean', pageNumbers: true, ...(s.options as object) }));
      break;
    }
    case 'photocalendar': {
      const imgs = await library.import(s.photos as string[]);
      openEditor(buildPhotoCalendar(imgs, { year: 2027, firstDay: 1, title: 'Our Year', ...(s.options as object) }));
      break;
    }
    case 'page':
      editor.goToPage(Number(s.index));
      break;
    case 'addPage':
      editor.addPage();
      break;
    case 'exportPages': {
      const pages = (s.pages as number[] | undefined) ?? Array.from({ length: editor.pageCount }, (_, i) => i);
      const res = await exportPages(editor.doc, pages, { path: s.path as string, format: (s.format as ExportFormat) ?? 'pdf', quality: Number(s.quality ?? 85), scale: Number(s.scale ?? 1) });
      await log(`exportPages ${JSON.stringify(res.map((r) => [r.path, r.width, r.height, r.bytes]))}`);
      break;
    }
    case 'print':
      void printProject('all');
      break;
    case 'log':
      await log(String(s.msg));
      break;
    case 'shot':
      // The harness takes a screenshot when it sees this line.
      await sleep(Number(s.settle ?? 1200));
      await log(`SHOT ${s.name}`);
      await sleep(1500);
      break;
    case 'state':
      await log(`state view=${app.view} elements=${editor.doc.elements.length} engine=${editor.doc.layout.engine} library=${library.items.length}`);
      break;
    case 'quit':
      await getCurrentWindow().close();
      break;
    default:
      throw new Error(`unknown step ${step.do}`);
  }
}

export async function maybeRunAutotest(): Promise<void> {
  let script: string | null = null;
  try {
    script = await invoke<string | null>('autotest_script');
  } catch {
    return;
  }
  if (!script) return;
  const steps = JSON.parse(script) as Step[];
  await log(`autotest: ${steps.length} steps`);
  for (const [i, step] of steps.entries()) {
    try {
      await log(`step ${i}: ${step.do}`);
      await run(step);
    } catch (e) {
      await log(`step ${i} FAILED: ${e}`);
    }
  }
  await log('autotest: done');
}
