// Client-side exports: Markdown and PowerPoint. (PDF goes through the print view; the
// offline package is built by the server.)
import { api, thumbUrl } from './api';
import { toHTML } from './tiptap';
import type { Doc, MediaRef, Resolved, Section } from './types';

function download(name: string, blob: Blob) {
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 10_000);
}

export function safeName(s: string) {
  return (s || 'document').replace(/[\\/:*?"<>|]+/g, '_').trim().slice(0, 120) || 'document';
}

function mediaURL(m: MediaRef) {
  return `${location.origin}/media/${m.libraryId}/${m.path.split('/').map(encodeURIComponent).join('/')}`;
}

// ---------- Markdown ----------

function inline(nodes: any[] | undefined): string {
  if (!nodes) return '';
  return nodes
    .map((n) => {
      if (n.type === 'hardBreak') return '  \n';
      if (n.type !== 'text') return inline(n.content);
      let t: string = n.text.replace(/([*_`[\]\\])/g, '\\$1');
      for (const m of n.marks ?? []) {
        if (m.type === 'bold') t = `**${t}**`;
        else if (m.type === 'italic') t = `*${t}*`;
        else if (m.type === 'strike') t = `~~${t}~~`;
        else if (m.type === 'code') t = '`' + n.text + '`';
        else if (m.type === 'highlight') t = `==${t}==`;
        else if (m.type === 'link') t = `[${t}](${m.attrs?.href ?? ''})`;
      }
      return t;
    })
    .join('');
}

function block(n: any, depth = 0, ordered = 0): string {
  const pad = '  '.repeat(depth);
  switch (n.type) {
    case 'paragraph':
      return inline(n.content);
    case 'heading':
      return '#'.repeat(Math.min(6, (n.attrs?.level ?? 1) + 1)) + ' ' + inline(n.content);
    case 'blockquote':
      return (n.content ?? []).map((c: any) => '> ' + block(c)).join('\n>\n');
    case 'codeBlock':
      return '```' + (n.attrs?.language ?? '') + '\n' + (n.content ?? []).map((c: any) => c.text ?? '').join('') + '\n```';
    case 'horizontalRule':
      return '---';
    case 'bulletList':
    case 'orderedList':
      return (n.content ?? [])
        .map((li: any, i: number) => {
          const mark = n.type === 'orderedList' ? `${(n.attrs?.start ?? 1) + i}.` : '-';
          const parts = (li.content ?? []).map((c: any, j: number) => (j === 0 ? block(c, depth + 1) : '\n' + pad + '  ' + block(c, depth + 1)));
          return pad + mark + ' ' + parts.join('');
        })
        .join('\n');
    case 'youtube':
      return `[▶ Video](${n.attrs?.src ?? ''})`;
    case 'mapEmbed':
      return `[📍 Map ${n.attrs?.lat}, ${n.attrs?.lon}](https://www.openstreetmap.org/?mlat=${n.attrs?.lat}&mlon=${n.attrs?.lon}#map=14/${n.attrs?.lat}/${n.attrs?.lon})`;
    default:
      return n.content ? (n.content as any[]).map((c) => block(c, depth, ordered)).join('\n\n') : '';
  }
}

export function sectionMarkdown(s: Section, i: number): string {
  const out: string[] = [];
  if (s.chapter) out.push(`# ${s.title || 'Chapter'}`);
  else if (s.title) out.push(`## ${s.title}`);
  else out.push(`## Section ${i + 1}`);
  for (const m of s.media) {
    const name = m.path.split('/').pop()!;
    out.push(`![${(m.caption || name).replace(/[[\]]/g, '')}](${mediaURL(m)})` + (m.caption ? `\n*${m.caption}*` : ''));
  }
  const body = (s.text?.content ?? []).map((n: any) => block(n)).filter((x: string) => x.trim()).join('\n\n');
  if (body) out.push(body);
  if (s.notes) out.push(`> **Notes:** ${s.notes.replace(/\n/g, '\n> ')}`);
  return out.join('\n\n');
}

export async function exportMarkdown(doc: Doc) {
  const head = [`# ${doc.title || 'Untitled'}`, ''];
  if (doc.tags?.length) head.push(doc.tags.map((t) => `#${t.replace(/\s+/g, '-')}`).join(' '), '');
  head.push(`*Exported from Tessera on ${new Date().toLocaleString()}. Media links point to this Tessera server (sign-in required).*`, '');
  const md = head.join('\n') + '\n' + doc.sections.map(sectionMarkdown).join('\n\n---\n\n') + '\n';
  download(safeName(doc.title) + '.md', new Blob([md], { type: 'text/markdown;charset=utf-8' }));
}

// ---------- PowerPoint ----------

/** Fetch an image as JPEG data (PowerPoint has no WebP support). */
async function jpegData(url: string, maxW = 1920): Promise<{ data: string; w: number; h: number } | null> {
  try {
    const img = new Image();
    img.decoding = 'async';
    img.src = url;
    await img.decode();
    const k = Math.min(1, maxW / img.naturalWidth);
    const c = document.createElement('canvas');
    c.width = Math.max(1, Math.round(img.naturalWidth * k));
    c.height = Math.max(1, Math.round(img.naturalHeight * k));
    c.getContext('2d')!.drawImage(img, 0, 0, c.width, c.height);
    return { data: c.toDataURL('image/jpeg', 0.88), w: c.width, h: c.height };
  } catch {
    return null;
  }
}

function plain(json: any): string {
  if (!json) return '';
  const lines: string[] = [];
  const walk = (n: any, prefix = '') => {
    if (n.type === 'paragraph' || n.type === 'heading') {
      const t = inlineText(n.content);
      if (t) lines.push(prefix + t);
    } else if (n.type === 'bulletList' || n.type === 'orderedList') {
      (n.content ?? []).forEach((li: any, i: number) => (li.content ?? []).forEach((c: any) => walk(c, n.type === 'orderedList' ? `${i + 1}. ` : '• ')));
    } else (n.content ?? []).forEach((c: any) => walk(c, prefix));
  };
  walk(json);
  return lines.join('\n');
}
function inlineText(nodes?: any[]): string {
  return (nodes ?? []).map((n) => (n.type === 'text' ? n.text : n.type === 'hardBreak' ? '\n' : inlineText(n.content))).join('');
}

/** fit (w,h) into a box keeping the ratio */
function fit(w: number, h: number, bw: number, bh: number) {
  const k = Math.min(bw / w, bh / h);
  return { w: w * k, h: h * k, dx: (bw - w * k) / 2, dy: (bh - h * k) / 2 };
}

export async function exportPptx(doc: Doc) {
  const { default: PptxGenJS } = await import('pptxgenjs');
  const pptx = new PptxGenJS();
  pptx.layout = 'LAYOUT_WIDE'; // 13.33 × 7.5 in
  pptx.title = doc.title;
  const W = 13.333,
    H = 7.5,
    M = 0.45;
  const slides = doc.sections.filter((s) => s.slide);
  const resolved = new Map<string, Resolved>();
  const refs = slides.flatMap((s) => s.media);
  for (let i = 0; i < refs.length; i += 400)
    for (const r of await api.resolve(refs.slice(i, i + 400).map((m) => ({ libraryId: m.libraryId, path: m.path })))) resolved.set(r.libraryId + '\u0000' + r.path, r);

  for (const s of slides) {
    const slide = pptx.addSlide();
    slide.background = { color: '0B0A0F' };
    const title = s.title || '';
    const text = plain(s.text);
    const media = s.layout === 'gallery' && s.gallery?.mode === 'carousel' ? s.media.slice(0, 1) : s.media;
    let top = M;
    if (s.chapter || (!media.length && !text)) {
      slide.addText(title || 'Chapter', { x: M, y: 0, w: W - 2 * M, h: H, fontSize: 44, bold: true, color: 'F2EFF7', align: 'center', valign: 'middle', fontFace: 'Georgia' });
    } else {
      if (title && s.layout !== 'media-full') {
        slide.addText(title, { x: M, y: M, w: W - 2 * M, h: 0.8, fontSize: 30, bold: true, color: 'F2EFF7', fontFace: 'Georgia' });
        top = M + 0.95;
      }
      const side = (s.layout === 'media-left' || s.layout === 'media-right') && text;
      let mx = M,
        my = top,
        mw = W - 2 * M,
        mh = H - top - M;
      let tx = M,
        ty = top,
        tw = W - 2 * M,
        th = 0;
      if (s.layout === 'media-full') {
        mx = 0;
        my = 0;
        mw = W;
        mh = H;
      } else if (side) {
        mw = (W - 3 * M) * 0.57;
        tw = W - 3 * M - mw;
        if (s.layout === 'media-left') tx = M * 2 + mw;
        else {
          tx = M;
          mx = M * 2 + tw;
        }
        th = mh;
      } else if (text && media.length) {
        th = Math.min(2.2, 0.35 * text.split('\n').length + 0.4);
        mh -= th + 0.2;
        ty = my + mh + 0.2;
      } else if (text) th = mh;

      if (media.length) {
        const cols = s.layout === 'gallery' ? s.gallery?.columns || Math.ceil(Math.sqrt(media.length)) : Math.min(media.length, 2);
        const rows = Math.ceil(media.length / cols);
        const gap = 0.12;
        const cw = (mw - gap * (cols - 1)) / cols,
          ch = (mh - gap * (rows - 1)) / rows;
        for (let i = 0; i < media.length; i++) {
          const m = media[i];
          const r = resolved.get(m.libraryId + '\u0000' + m.path);
          if (!r?.found || !r.file) continue;
          const img = await jpegData(thumbUrl(r.derived, 2560), 2400);
          if (!img) continue;
          const cx = mx + (i % cols) * (cw + gap),
            cy = my + Math.floor(i / cols) * (ch + gap);
          const f = fit(img.w, img.h, cw, ch);
          const isVideo = r.file.kind === 'video';
          slide.addImage({
            data: img.data,
            x: cx + f.dx,
            y: cy + f.dy,
            w: f.w,
            h: f.h,
            hyperlink: isVideo ? { url: `${location.origin}${r.original}`, tooltip: 'Play the video (Tessera server)' } : undefined,
          });
          if (isVideo)
            slide.addText('▶', { x: cx + f.dx + f.w / 2 - 0.5, y: cy + f.dy + f.h / 2 - 0.5, w: 1, h: 1, fontSize: 40, color: 'FFFFFF', align: 'center', valign: 'middle', shape: pptx.ShapeType.ellipse, fill: { color: '000000', transparency: 45 } });
          if (m.caption && s.layout !== 'media-full')
            slide.addText(m.caption, { x: cx, y: cy + ch - 0.35, w: cw, h: 0.35, fontSize: 12, italic: true, color: 'CFCBDB', align: 'center' });
        }
      }
      if (text && s.layout !== 'media-full') slide.addText(text, { x: tx, y: ty, w: tw, h: th, fontSize: 16, color: 'E9E6F0', valign: 'top', fontFace: 'Georgia', fit: 'shrink' });
      if (s.layout === 'media-full' && (title || text))
        slide.addText([title, text].filter(Boolean).join('\n'), { x: M, y: H - 1.9, w: W - 2 * M, h: 1.5, fontSize: 22, color: 'FFFFFF', bold: !!title, fill: { color: '000000', transparency: 40 } });
    }
    if (s.notes) slide.addNotes(s.notes);
  }
  if (!slides.length) pptx.addSlide().addText('No sections are marked for the presentation.', { x: 1, y: 3, w: 11, h: 1, fontSize: 24 });
  const blob = (await pptx.write({ outputType: 'blob' })) as Blob;
  download(safeName(doc.title) + '.pptx', blob);
}

// ---------- offline package ----------

/** Ask the server for the zip; the slides' rich text is rendered here (the editor owns it). */
export function exportOffline(doc: Doc, quality: 'light' | 'original' = 'light') {
  const html: Record<string, string> = {};
  for (const s of doc.sections) if (s.slide) html[s.id] = toHTML(s.text);
  const form = document.createElement('form');
  form.method = 'POST';
  form.action = `/api/documents/${doc.id}/offline`;
  form.style.display = 'none';
  const input = document.createElement('input');
  input.type = 'hidden';
  input.name = 'payload';
  input.value = JSON.stringify({ html, quality });
  form.appendChild(input);
  document.body.appendChild(form);
  form.submit();
  form.remove();
}
