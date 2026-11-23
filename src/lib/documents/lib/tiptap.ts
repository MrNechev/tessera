import { generateHTML, mergeAttributes, Node, type Extensions } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import TextAlign from '@tiptap/extension-text-align';
import Highlight from '@tiptap/extension-highlight';
import Youtube from '@tiptap/extension-youtube';
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight';
import { Placeholder } from '@tiptap/extensions';
import { common, createLowlight } from 'lowlight';

const lowlight = createLowlight(common);

/** An OpenStreetMap embed (inserted from a photo's GPS position or typed coordinates). */
export const MapEmbed = Node.create({
  name: 'mapEmbed',
  group: 'block',
  atom: true,
  draggable: true,
  addAttributes() {
    return {
      lat: { default: 0 },
      lon: { default: 0 },
      zoom: { default: 13 },
      label: { default: '' },
    };
  },
  parseHTML() {
    return [{ tag: 'div[data-map-embed]' }];
  },
  renderHTML({ HTMLAttributes }) {
    const lat = Number(HTMLAttributes.lat) || 0,
      lon = Number(HTMLAttributes.lon) || 0,
      z = Number(HTMLAttributes.zoom) || 13;
    const d = 180 / Math.pow(2, z); // rough bbox half-size in degrees for the zoom
    const bbox = [lon - d, lat - d / 2, lon + d, lat + d / 2].map((v) => v.toFixed(5)).join(',');
    const src = `https://www.openstreetmap.org/export/embed.html?bbox=${bbox}&layer=mapnik&marker=${lat},${lon}`;
    return [
      'div',
      mergeAttributes({ 'data-map-embed': '', class: 'map-embed', 'data-lat': lat, 'data-lon': lon, 'data-zoom': z, 'data-label': HTMLAttributes.label }),
      ['iframe', { src, loading: 'lazy', referrerpolicy: 'strict-origin-when-cross-origin', title: HTMLAttributes.label || 'Map' }],
      ['a', { href: `https://www.openstreetmap.org/?mlat=${lat}&mlon=${lon}#map=${z}/${lat}/${lon}`, target: '_blank', rel: 'noopener noreferrer' }, HTMLAttributes.label || `${lat.toFixed(5)}, ${lon.toFixed(5)}`],
    ];
  },
  addCommands() {
    return {
      setMap:
        (attrs: { lat: number; lon: number; zoom?: number; label?: string }) =>
        ({ commands }: any) =>
          commands.insertContent({ type: this.name, attrs }),
    } as any;
  },
});

export function extensions(placeholder = 'Write something…'): Extensions {
  return [
    StarterKit.configure({
      heading: { levels: [1, 2, 3] },
      codeBlock: false,
      link: { openOnClick: false, autolink: true, HTMLAttributes: { rel: 'noopener noreferrer', target: '_blank' } },
    }),
    CodeBlockLowlight.configure({ lowlight, defaultLanguage: null }),
    Youtube.configure({ nocookie: true, modestBranding: true, width: 640, height: 360, HTMLAttributes: { referrerpolicy: 'strict-origin-when-cross-origin', class: 'yt-embed' } }),
    MapEmbed,
    TextAlign.configure({ types: ['heading', 'paragraph'] }),
    Highlight,
    Placeholder.configure({ placeholder }),
  ];
}

const staticExt = extensions();
const htmlCache = new WeakMap<object, string>();

/** Static HTML for sections whose editor isn't mounted (far from the viewport, presentation). */
export function toHTML(json: any): string {
  if (!json || typeof json !== 'object') return '';
  const hit = htmlCache.get(json);
  if (hit !== undefined) return hit;
  let html = '';
  try {
    html = generateHTML(json, staticExt);
  } catch {
    html = '';
  }
  htmlCache.set(json, html);
  return html;
}

export function isEmptyText(json: any): boolean {
  if (!json?.content?.length) return true;
  return json.content.every((n: any) => n.type === 'paragraph' && !n.content?.length);
}
