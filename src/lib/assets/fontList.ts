/* Generated from public/fonts/fonts.json — do not edit by hand. */

export type FontCategory = 'sans' | 'rounded' | 'display' | 'serif' | 'script' | 'handwriting' | 'themed';

export interface FontInfo {
  id: string;
  family: string;
  category: FontCategory;
  weights: number[];
  license: string;
}

export const FONT_CATEGORIES: { id: FontCategory; name: string }[] = [
  { id: 'sans', name: 'Sans serif' },
  { id: 'rounded', name: 'Rounded' },
  { id: 'display', name: 'Display' },
  { id: 'serif', name: 'Serif' },
  { id: 'script', name: 'Script' },
  { id: 'handwriting', name: 'Handwriting' },
  { id: 'themed', name: 'Themed' },
];

export const FONTS: FontInfo[] = [
  { id: 'inter', family: 'Inter', category: 'sans', weights: [400, 500, 600, 700, 800], license: 'OFL-1.1' },
  { id: 'montserrat', family: 'Montserrat', category: 'sans', weights: [400, 600, 800], license: 'OFL-1.1' },
  { id: 'poppins', family: 'Poppins', category: 'sans', weights: [400, 600, 800], license: 'OFL-1.1' },
  { id: 'josefin-sans', family: 'Josefin Sans', category: 'sans', weights: [300, 400, 700], license: 'OFL-1.1' },
  { id: 'fredoka', family: 'Fredoka', category: 'rounded', weights: [400, 600], license: 'OFL-1.1' },
  { id: 'comfortaa', family: 'Comfortaa', category: 'rounded', weights: [400, 700], license: 'OFL-1.1' },
  { id: 'oswald', family: 'Oswald', category: 'display', weights: [400, 600], license: 'OFL-1.1' },
  { id: 'bebas-neue', family: 'Bebas Neue', category: 'display', weights: [400], license: 'OFL-1.1' },
  { id: 'righteous', family: 'Righteous', category: 'display', weights: [400], license: 'OFL-1.1' },
  { id: 'abril-fatface', family: 'Abril Fatface', category: 'display', weights: [400], license: 'OFL-1.1' },
  { id: 'shrikhand', family: 'Shrikhand', category: 'display', weights: [400], license: 'OFL-1.1' },
  { id: 'playfair-display', family: 'Playfair Display', category: 'serif', weights: [400, 700], license: 'OFL-1.1' },
  { id: 'lora', family: 'Lora', category: 'serif', weights: [400, 700], license: 'OFL-1.1' },
  { id: 'cinzel', family: 'Cinzel', category: 'serif', weights: [400, 700], license: 'OFL-1.1' },
  { id: 'pacifico', family: 'Pacifico', category: 'script', weights: [400], license: 'OFL-1.1' },
  { id: 'dancing-script', family: 'Dancing Script', category: 'script', weights: [400, 700], license: 'OFL-1.1' },
  { id: 'great-vibes', family: 'Great Vibes', category: 'script', weights: [400], license: 'OFL-1.1' },
  { id: 'sacramento', family: 'Sacramento', category: 'script', weights: [400], license: 'OFL-1.1' },
  { id: 'satisfy', family: 'Satisfy', category: 'script', weights: [400], license: 'Apache-2.0' },
  { id: 'lobster', family: 'Lobster', category: 'script', weights: [400], license: 'OFL-1.1' },
  { id: 'caveat', family: 'Caveat', category: 'handwriting', weights: [400, 700], license: 'OFL-1.1' },
  { id: 'indie-flower', family: 'Indie Flower', category: 'handwriting', weights: [400], license: 'OFL-1.1' },
  { id: 'amatic-sc', family: 'Amatic SC', category: 'handwriting', weights: [400, 700], license: 'OFL-1.1' },
  { id: 'permanent-marker', family: 'Permanent Marker', category: 'handwriting', weights: [400], license: 'Apache-2.0' },
  { id: 'mountains-of-christmas', family: 'Mountains of Christmas', category: 'themed', weights: [400, 700], license: 'Apache-2.0' },
  { id: 'creepster', family: 'Creepster', category: 'themed', weights: [400], license: 'OFL-1.1' },
];
