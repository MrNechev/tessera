// A plain diagram graph: what the Mermaid and draw.io importers produce and what
// build.ts turns into canvas items (laid out with dagre when there are no positions).
import type { Dash, Head, Route } from '../types';

/** outline of an imported node; 'rect' / 'ellipse' become plain shapes, 'text' free text */
export type NodeShape =
  | 'rect'
  | 'ellipse'
  | 'diamond'
  | 'triangle'
  | 'parallelogram'
  | 'hexagon'
  | 'cylinder'
  | 'document'
  | 'cloud'
  | 'callout'
  | 'star'
  | 'text';

export type Direction = 'TB' | 'BT' | 'LR' | 'RL';

export interface GraphNode {
  id: string;
  label: string;
  shape: NodeShape;
  /** rounded corners (rect only) */
  rounded?: boolean;
  /** a stadium / pill (rect with half-height corners) */
  pill?: boolean;
  /** id of the group (subgraph / container) it belongs to */
  parent?: string;
  /** position and size when the source has them (draw.io); world px */
  x?: number;
  y?: number;
  w?: number;
  h?: number;
  /** '#rrggbb' colours when the source has them */
  fill?: string;
  stroke?: string;
  fontColor?: string;
  strokeWidth?: number;
  dash?: Dash;
  fontSize?: number;
}

export interface GraphEdge {
  id?: string;
  /** node ids; missing = a free end at fromPoint / toPoint */
  from?: string;
  to?: string;
  fromPoint?: { x: number; y: number };
  toPoint?: { x: number; y: number };
  label?: string;
  start?: Head;
  end?: Head;
  dash?: Dash;
  /** line width (thick Mermaid links ==> are 5) */
  width?: number;
  stroke?: string;
  route?: Route;
}

export interface GraphGroup {
  id: string;
  label: string;
  parent?: string;
  x?: number;
  y?: number;
  w?: number;
  h?: number;
}

export interface DiagramGraph {
  direction: Direction;
  nodes: GraphNode[];
  edges: GraphEdge[];
  groups: GraphGroup[];
  /** every node has x/y/w/h (draw.io): keep them instead of laying out */
  positioned?: boolean;
}
