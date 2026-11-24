// Starting points for new documents. Text uses TipTap JSON; media is left for the user.
import { newId } from './api';
import type { Doc, Section } from './types';

const p = (text: string) => ({ type: 'paragraph', content: text ? [{ type: 'text', text }] : undefined });
const doc = (...content: any[]) => ({ type: 'doc', content });
const sec = (o: Partial<Section>): Section => ({ id: newId(), title: '', layout: 'media-top', media: [], text: doc(p('')), slide: true, ...o });

export interface Template {
  id: string;
  name: string;
  icon: string;
  description: string;
  canvas?: boolean;
  build: () => Pick<Doc, 'title' | 'sections'> & Partial<Doc>;
}

export const TEMPLATES: Template[] = [
  {
    id: 'trip',
    name: 'Trip diary',
    icon: 'map',
    description: 'A chapter per day, sections for places, a map and the route.',
    build: () => ({
      title: 'Trip diary',
      tags: ['trip'],
      sections: [
        sec({ title: 'The trip', layout: 'media-full', text: doc(p('Where, when and who. Drop a cover photo here.')) }),
        sec({ title: 'Day 1', chapter: true, layout: 'text-only', text: doc(p('Arrival.')) }),
        sec({ title: 'Arrival', text: doc(p('First impressions. Add photos from the library, then use Story settings → Tools → “Add date & place captions”.')) }),
        sec({ title: 'Highlight of the day', layout: 'media-left', text: doc(p('What made the day.')) }),
        sec({ title: 'Day 2', chapter: true, layout: 'text-only', text: doc(p('')) }),
        sec({ title: 'Gallery', layout: 'gallery', gallery: { mode: 'grid', columns: 3 }, text: doc(p('')) }),
        sec({ title: 'Looking back', layout: 'text-only', text: doc(p('Favourite moment, what to do differently next time.')) }),
      ],
    }),
  },
  {
    id: 'tutorial',
    name: 'Tutorial',
    icon: 'notes',
    description: 'Goal, what you need, numbered steps with a picture each, result.',
    build: () => ({
      title: 'How to …',
      tags: ['tutorial'],
      sections: [
        sec({ title: 'What we will make', layout: 'media-top', text: doc(p('The end result, in one picture and one sentence.')) }),
        sec({ title: 'You will need', layout: 'text-only', text: doc({ type: 'bulletList', content: ['Tool', 'Material', 'Time'].map((t) => ({ type: 'listItem', content: [p(t)] })) }) }),
        ...[1, 2, 3].map((n) => sec({ title: `Step ${n}`, layout: 'media-left', text: doc(p('What to do, and what to watch out for.')) })),
        sec({ title: 'Commands', layout: 'text-only', text: doc(p('Code or commands, highlighted:'), { type: 'codeBlock', attrs: { language: 'bash' }, content: [{ type: 'text', text: 'echo "hello"' }] }) }),
        sec({ title: 'Result', layout: 'media-full', text: doc(p('')) }),
      ],
    }),
  },
  {
    id: 'compare',
    name: 'Before / after',
    icon: 'compare',
    description: 'Comparison slides: drag a slider between two photos or play two videos in sync.',
    build: () => ({
      title: 'Before and after',
      sections: [
        sec({ title: 'Before and after', layout: 'text-only', text: doc(p('Add two photos to each comparison section: the first is “before”, the second “after”.')) }),
        sec({ title: 'Comparison 1', layout: 'compare', compare: { mode: 'slider', pos: 0.5 } }),
        sec({ title: 'Comparison 2', layout: 'compare', compare: { mode: 'slider', pos: 0.5 } }),
        sec({ title: 'Side by side', layout: 'compare', compare: { mode: 'sync' }, text: doc(p('Two videos playing together.')) }),
      ],
    }),
  },
  {
    id: 'mood',
    name: 'Mood board',
    icon: 'canvas',
    canvas: true,
    description: 'A canvas with frames for themes — drag media straight onto it.',
    build: () => {
      const f = (title: string, x: number, y: number) => ({ id: newId(), type: 'frame' as const, x, y, w: 1600, h: 900, title });
      return {
        title: 'Mood board',
        tags: ['mood board'],
        sections: [],
        canvas: {
          items: [
            f('Colours', 0, 0),
            f('Textures', 1760, 0),
            f('Places', 0, 1060),
            f('People', 1760, 1060),
            { id: newId(), type: 'note', x: 40, y: -260, w: 640, h: 140, text: 'Drag images from the library into the frames.\nFrames become the steps of the presentation.', color: 'yellow', size: 28 },
          ],
        },
      };
    },
  },
];

export function fromTemplate(t: Template): Partial<Doc> {
  return { ...t.build(), id: undefined as unknown as string };
}
