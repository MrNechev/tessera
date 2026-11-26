import { describe, expect, it } from 'vitest';
import { parseMermaid } from './mermaid';
import type { GraphNode } from './graph';

const node = (text: string, id = 'A'): GraphNode => {
  const n = parseMermaid(`flowchart TD\n${text}`).nodes.find((x) => x.id === id);
  if (!n) throw new Error(`no node ${id}`);
  return n;
};

const edge = (link: string) => {
  const g = parseMermaid(`graph LR\nA ${link} B`);
  expect(g.edges).toHaveLength(1);
  expect(g.edges[0]).toMatchObject({ from: 'A', to: 'B' });
  return g.edges[0];
};

describe('parseMermaid: header', () => {
  it('reads the direction', () => {
    expect(parseMermaid('flowchart TD\nA').direction).toBe('TB');
    expect(parseMermaid('flowchart TB\nA').direction).toBe('TB');
    expect(parseMermaid('graph LR\nA').direction).toBe('LR');
    expect(parseMermaid('graph RL\nA').direction).toBe('RL');
    expect(parseMermaid('graph BT\nA').direction).toBe('BT');
    expect(parseMermaid('flowchart\nA').direction).toBe('TB');
    expect(parseMermaid('  graph   lr  \n A').direction).toBe('LR');
  });

  it('skips front matter, comments and init directives', () => {
    const g = parseMermaid('---\ntitle: Hello\n---\n%%{init: {"theme": "dark"}}%%\n%% a comment\nflowchart LR\n  %% another\n  A --> B');
    expect(g.direction).toBe('LR');
    expect(g.nodes.map((n) => n.id)).toEqual(['A', 'B']);
    expect(g.edges).toHaveLength(1);
  });

  it('throws a friendly error on other text', () => {
    expect(() => parseMermaid('sequenceDiagram\nAlice->>Bob: Hi')).toThrow(/Not a Mermaid flowchart/);
    expect(() => parseMermaid('')).toThrow(/flowchart TD/);
    expect(() => parseMermaid('hello world')).toThrow(Error);
  });
});

describe('parseMermaid: node shapes', () => {
  it('reads every bracket shape', () => {
    expect(node('A[Box]')).toEqual({ id: 'A', label: 'Box', shape: 'rect' });
    expect(node('A(Round)')).toEqual({ id: 'A', label: 'Round', shape: 'rect', rounded: true });
    expect(node('A([Pill])')).toEqual({ id: 'A', label: 'Pill', shape: 'rect', pill: true });
    expect(node('A[[Sub]]')).toEqual({ id: 'A', label: 'Sub', shape: 'rect' });
    expect(node('A[(DB)]')).toMatchObject({ label: 'DB', shape: 'cylinder' });
    expect(node('A((Circle))')).toMatchObject({ label: 'Circle', shape: 'ellipse' });
    expect(node('A(((Double)))')).toMatchObject({ label: 'Double', shape: 'ellipse' });
    expect(node('A{Choice?}')).toMatchObject({ label: 'Choice?', shape: 'diamond' });
    expect(node('A{{Hex}}')).toMatchObject({ label: 'Hex', shape: 'hexagon' });
    expect(node('A[/In/]')).toMatchObject({ label: 'In', shape: 'parallelogram' });
    expect(node('A[\\Out\\]')).toMatchObject({ label: 'Out', shape: 'parallelogram' });
    expect(node('A[/Trap\\]')).toMatchObject({ label: 'Trap', shape: 'parallelogram' });
    expect(node('A[\\Trap/]')).toMatchObject({ label: 'Trap', shape: 'parallelogram' });
    expect(node('A>Flag]')).toMatchObject({ label: 'Flag', shape: 'callout' });
  });

  it('uses the id for bare nodes', () => {
    expect(node('Start', 'Start')).toEqual({ id: 'Start', label: 'Start', shape: 'rect' });
  });

  it('reads quoted labels, <br> and entities', () => {
    expect(node('A["Text with (parens) and [brackets]"]').label).toBe('Text with (parens) and [brackets]');
    expect(node('A("f(x) = y")').label).toBe('f(x) = y');
    expect(node('A[One<br>Two<br/>Three<br />Four]').label).toBe('One\nTwo\nThree\nFour');
    expect(node('A["Say #quot;hi#quot; &amp; #35;1"]').label).toBe('Say "hi" & #1');
    expect(node('A["`Markdown`"]').label).toBe('Markdown');
    expect(node('A[  spaced  ]').label).toBe('spaced');
  });

  it('keeps the first definition when mentioned again bare, updates on a new shape', () => {
    let g = parseMermaid('graph TD\nA{Choose} --> B\nB --> A');
    expect(g.nodes[0]).toMatchObject({ id: 'A', label: 'Choose', shape: 'diamond' });
    g = parseMermaid('graph TD\nA --> B\nA((Now round))');
    expect(g.nodes[0]).toEqual({ id: 'A', label: 'Now round', shape: 'ellipse' });
    g = parseMermaid('graph TD\nA(Rounded)\nA[Square]');
    expect(g.nodes[0]).toEqual({ id: 'A', label: 'Square', shape: 'rect' });
  });

  it('accepts ids with dashes, digits, unicode and :::class', () => {
    const g = parseMermaid('graph LR\nnode-1[First]-->node-2\nПривет --> café_2:::hot\nx1 --> y');
    expect(g.nodes.map((n) => n.id)).toEqual(['node-1', 'node-2', 'Привет', 'café_2', 'x1', 'y']);
    expect(g.edges.map((e) => [e.from, e.to])).toEqual([
      ['node-1', 'node-2'],
      ['Привет', 'café_2'],
      ['x1', 'y'],
    ]);
  });

  it('reads the @{ shape } syntax', () => {
    expect(node('A@{ shape: diam, label: "Pick one" }')).toMatchObject({ id: 'A', label: 'Pick one', shape: 'diamond' });
    expect(node('A@{ shape: unknown-thing }')).toEqual({ id: 'A', label: 'A', shape: 'rect' });
  });
});

describe('parseMermaid: links', () => {
  it('reads every link type', () => {
    expect(edge('-->')).toEqual({ from: 'A', to: 'B', start: 'none', end: 'arrow' });
    expect(edge('---')).toEqual({ from: 'A', to: 'B', start: 'none', end: 'none' });
    expect(edge('-.->')).toEqual({ from: 'A', to: 'B', start: 'none', end: 'arrow', dash: 'dotted' });
    expect(edge('-.-')).toEqual({ from: 'A', to: 'B', start: 'none', end: 'none', dash: 'dotted' });
    expect(edge('==>')).toEqual({ from: 'A', to: 'B', start: 'none', end: 'arrow', width: 5 });
    expect(edge('===')).toEqual({ from: 'A', to: 'B', start: 'none', end: 'none', width: 5 });
    expect(edge('--o')).toMatchObject({ start: 'none', end: 'dot' });
    expect(edge('--x')).toMatchObject({ start: 'none', end: 'bar' });
    expect(edge('<-->')).toMatchObject({ start: 'arrow', end: 'arrow' });
    expect(edge('o--o')).toMatchObject({ start: 'dot', end: 'dot' });
    expect(edge('x--x')).toMatchObject({ start: 'bar', end: 'bar' });
    expect(edge('<-.->')).toMatchObject({ start: 'arrow', end: 'arrow', dash: 'dotted' });
    expect(edge('<==>')).toMatchObject({ start: 'arrow', end: 'arrow', width: 5 });
  });

  it('reads longer links the same way', () => {
    expect(edge('--->')).toMatchObject({ end: 'arrow' });
    expect(edge('-----')).toMatchObject({ end: 'none' });
    expect(edge('-..->')).toMatchObject({ end: 'arrow', dash: 'dotted' });
    expect(edge('====>')).toMatchObject({ end: 'arrow', width: 5 });
  });

  it('works without spaces around links', () => {
    const g = parseMermaid('graph TD\nA-->B---C-.->D==>E');
    expect(g.edges.map((e) => `${e.from}${e.to}`)).toEqual(['AB', 'BC', 'CD', 'DE']);
  });

  it('reads |pipe| labels', () => {
    expect(edge('-->|Yes|')).toMatchObject({ label: 'Yes', end: 'arrow' });
    expect(edge('-.->|"maybe (later)"|')).toMatchObject({ label: 'maybe (later)', dash: 'dotted' });
    expect(edge('==> |Big|')).toMatchObject({ label: 'Big', width: 5 });
    expect(edge('---|plain|')).toMatchObject({ label: 'plain', end: 'none' });
  });

  it('reads inline text labels', () => {
    expect(edge('-- text -->')).toMatchObject({ label: 'text', start: 'none', end: 'arrow' });
    expect(edge('-- line text ---')).toMatchObject({ label: 'line text', end: 'none' });
    expect(edge('-. dotted text .->')).toMatchObject({ label: 'dotted text', end: 'arrow', dash: 'dotted' });
    expect(edge('== thick text ==>')).toMatchObject({ label: 'thick text', end: 'arrow', width: 5 });
    expect(edge('<-- both -->')).toMatchObject({ label: 'both', start: 'arrow', end: 'arrow' });
    expect(edge('-- "quoted" --o')).toMatchObject({ label: 'quoted', end: 'dot' });
  });

  it('reads chains and & groups', () => {
    let g = parseMermaid('graph LR\nA --> B --> C');
    expect(g.edges.map((e) => `${e.from}${e.to}`)).toEqual(['AB', 'BC']);
    g = parseMermaid('graph LR\nA & B --> C & D');
    expect(g.edges.map((e) => `${e.from}${e.to}`)).toEqual(['AC', 'AD', 'BC', 'BD']);
    g = parseMermaid('graph LR\nA[Start] -->|go| B{Ok?} -- yes --> C([Done]) & D');
    expect(g.edges.map((e) => [`${e.from}${e.to}`, e.label])).toEqual([
      ['AB', 'go'],
      ['BC', 'yes'],
      ['BD', 'yes'],
    ]);
    expect(g.nodes.map((n) => n.shape)).toEqual(['rect', 'diamond', 'rect', 'rect']);
    expect(g.nodes[2].pill).toBe(true);
  });

  it('drops invisible ~~~ links but keeps their nodes', () => {
    const g = parseMermaid('graph LR\nA ~~~ B');
    expect(g.nodes).toHaveLength(2);
    expect(g.edges).toHaveLength(0);
  });
});

describe('parseMermaid: statements and subgraphs', () => {
  it('splits on semicolons and ignores styling statements', () => {
    const g = parseMermaid(
      'graph TD;A["x;y"]-->B;B-->|a;b|C;\nclassDef hot fill:#f00;\nclass A hot\nstyle B fill:#0f0\nlinkStyle 0 stroke:#f00\nclick A callback\n',
    );
    expect(g.nodes.map((n) => [n.id, n.label])).toEqual([
      ['A', 'x;y'],
      ['B', 'B'],
      ['C', 'C'],
    ]);
    expect(g.edges.map((e) => e.label)).toEqual([undefined, 'a;b']);
  });

  it('reads subgraphs, titles and nesting', () => {
    const g = parseMermaid(`flowchart TB
      c1-->a2
      subgraph one [First group]
        direction LR
        a1-->a2
      end
      subgraph two["Second"]
        b1-->b2
        subgraph inner
          x
        end
      end
      subgraph Title with spaces
        t1
      end
      subgraph
        u
      end
      c1-->b1`);
    expect(g.groups).toEqual([
      { id: 'one', label: 'First group' },
      { id: 'two', label: 'Second' },
      { id: 'inner', label: 'inner', parent: 'two' },
      { id: 'subgraph-4', label: 'Title with spaces' },
      { id: 'subgraph-5', label: '' },
    ]);
    const parents = Object.fromEntries(g.nodes.map((n) => [n.id, n.parent]));
    expect(parents).toEqual({ c1: undefined, a2: 'one', a1: 'one', b1: 'two', b2: 'two', x: 'inner', t1: 'subgraph-4', u: 'subgraph-5' });
    expect(g.edges).toHaveLength(4);
  });

  it('drops edges to subgraph ids', () => {
    const g = parseMermaid('graph LR\nsubgraph S\n  a --> b\nend\nc --> S\nS --> d\nc --> a');
    expect(g.nodes.map((n) => n.id)).toEqual(['a', 'b', 'c', 'd']);
    expect(g.edges.map((e) => `${e.from}${e.to}`)).toEqual(['ab', 'ca']);
  });

  it('reads a realistic flowchart', () => {
    const g = parseMermaid(`
      %% Order handling
      flowchart LR
        start([Order received]) --> check{In stock?}
        check -->|Yes| pack[Pack items]
        check -->|No| backorder[/Backorder form/]
        backorder -.-> notify>Email customer]
        subgraph warehouse [Warehouse]
          pack --> label[[Print label]]
          label --> ship[(Shipping DB)]
        end
        ship ==> done((Done))
        notify --- done
        classDef warn fill:#fee
        class backorder warn
        style done stroke-width:4px
    `);
    expect(g.direction).toBe('LR');
    expect(g.nodes.map((n) => [n.id, n.shape])).toEqual([
      ['start', 'rect'],
      ['check', 'diamond'],
      ['pack', 'rect'],
      ['backorder', 'parallelogram'],
      ['notify', 'callout'],
      ['label', 'rect'],
      ['ship', 'cylinder'],
      ['done', 'ellipse'],
    ]);
    expect(g.nodes.find((n) => n.id === 'start')).toMatchObject({ label: 'Order received', pill: true });
    expect(g.nodes.filter((n) => n.parent === 'warehouse').map((n) => n.id)).toEqual(['pack', 'label', 'ship']);
    expect(g.groups).toEqual([{ id: 'warehouse', label: 'Warehouse' }]);
    expect(g.edges).toEqual([
      { from: 'start', to: 'check', start: 'none', end: 'arrow' },
      { from: 'check', to: 'pack', start: 'none', end: 'arrow', label: 'Yes' },
      { from: 'check', to: 'backorder', start: 'none', end: 'arrow', label: 'No' },
      { from: 'backorder', to: 'notify', start: 'none', end: 'arrow', dash: 'dotted' },
      { from: 'pack', to: 'label', start: 'none', end: 'arrow' },
      { from: 'label', to: 'ship', start: 'none', end: 'arrow' },
      { from: 'ship', to: 'done', start: 'none', end: 'arrow', width: 5 },
      { from: 'notify', to: 'done', start: 'none', end: 'none' },
    ]);
  });
});
