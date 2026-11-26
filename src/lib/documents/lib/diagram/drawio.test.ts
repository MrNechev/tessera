import { describe, expect, it } from 'vitest';
import { deflateSync, strToU8 } from 'fflate';
import { parseDrawio } from './drawio';

const MODEL = `<mxGraphModel dx="1000" dy="800" grid="1">
  <root>
    <mxCell id="0" />
    <mxCell id="1" parent="0" />
    <!-- plain shapes -->
    <mxCell id="box" value="Box" style="rounded=1;whiteSpace=wrap;html=1;fillColor=#DAE8FC;strokeColor=#6c8ebf;fontColor=#333;strokeWidth=2;fontSize=14;" vertex="1" parent="1">
      <mxGeometry x="40" y="60" width="120" height="60" as="geometry" />
    </mxCell>
    <mxCell id="oval" value="Oval" style="ellipse;whiteSpace=wrap;html=1;dashed=1;fillColor=none;strokeColor=default;" vertex="1" parent="1">
      <mxGeometry x="240" y="60" width="80" height="80" as="geometry" />
    </mxCell>
    <mxCell id="dec" value="Ok?" style="rhombus;whiteSpace=wrap;html=1;dashed=1;dashPattern=1 2;" vertex="1" parent="1">
      <mxGeometry y="200" width="80" height="80" as="geometry" />
    </mxCell>
    <mxCell id="note" value="Just text" style="text;html=1;align=center;" vertex="1" parent="1">
      <mxGeometry x="400" y="20" width="60" height="30" as="geometry" />
    </mxCell>
    <!-- a swimlane with two children in relative coordinates; listed after them -->
    <mxCell id="c1" value="Child 1" style="whiteSpace=wrap;" vertex="1" parent="lane">
      <mxGeometry x="10" y="30" width="80" height="40" as="geometry" />
    </mxCell>
    <mxCell id="c2" value="Child 2" style="shape=cylinder3;whiteSpace=wrap;" vertex="1" parent="lane">
      <mxGeometry x="10" y="90" width="80" height="40" as="geometry" />
    </mxCell>
    <mxCell id="lane" value="Lane" style="swimlane;whiteSpace=wrap;html=1;" vertex="1" parent="1">
      <mxGeometry x="500" y="100" width="200" height="200" as="geometry" />
    </mxCell>
    <object label="Wrapped &amp; tagged" tag="x" id="obj">
      <mxCell style="shape=hexagon;whiteSpace=wrap;" vertex="1" parent="1">
        <mxGeometry x="800" y="0" width="100" height="50" as="geometry" />
      </mxCell>
    </object>
    <!-- edges -->
    <mxCell id="e1" value="goes &lt;b&gt;to&lt;/b&gt;" style="edgeStyle=orthogonalEdgeStyle;rounded=0;html=1;dashed=1;endArrow=open;startArrow=oval;strokeColor=#FF0000;strokeWidth=3;" edge="1" parent="1" source="box" target="oval">
      <mxGeometry relative="1" as="geometry" />
    </mxCell>
    <mxCell id="e2" style="endArrow=none;" edge="1" parent="1" source="dec">
      <mxGeometry relative="1" as="geometry">
        <mxPoint x="5" y="6" as="sourcePoint" />
        <mxPoint x="300" y="400" as="targetPoint" />
      </mxGeometry>
    </mxCell>
    <mxCell id="e3" style="curved=1;" edge="1" parent="1" source="obj" target="c2">
      <mxGeometry relative="1" as="geometry" />
    </mxCell>
    <mxCell id="e4" value="" style="edgeStyle=orthogonalEdgeStyle;" edge="1" parent="1" source="c1" target="lane">
      <mxGeometry relative="1" as="geometry" />
    </mxCell>
    <mxCell id="e4-label" value="Label&lt;br&gt;cell" style="edgeLabel;html=1;align=center;" vertex="1" connectable="0" parent="e4">
      <mxGeometry x="-0.2" relative="1" as="geometry"><mxPoint as="offset" /></mxGeometry>
    </mxCell>
    <mxCell id="e5" edge="1" parent="1" source="box">
      <mxGeometry relative="1" as="geometry" />
    </mxCell>
  </root>
</mxGraphModel>`;

const FILE = `<?xml version="1.0" encoding="UTF-8"?>
<mxfile host="app.diagrams.net" version="24.0.0">
  <diagram name="Page-1" id="p1">
    ${MODEL}
  </diagram>
  <diagram name="Page-2" id="p2">
    <mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="other" vertex="1" parent="1"><mxGeometry width="1" height="1" as="geometry"/></mxCell></root></mxGraphModel>
  </diagram>
</mxfile>`;

const compress = (xml: string) => Buffer.from(deflateSync(strToU8(encodeURIComponent(xml)))).toString('base64');

describe('parseDrawio', () => {
  const g = parseDrawio(FILE);
  const byId = (id: string) => g.nodes.find((n) => n.id === id);

  it('reads only the first page and skips layers', () => {
    expect(g.nodes.map((n) => n.id)).toEqual(['box', 'oval', 'dec', 'note', 'c1', 'c2', 'obj']);
    expect(g.direction).toBe('TB');
    expect(g.positioned).toBe(true);
  });

  it('reads vertex shapes, geometry and style', () => {
    expect(byId('box')).toEqual({
      id: 'box',
      label: 'Box',
      shape: 'rect',
      rounded: true,
      x: 40,
      y: 60,
      w: 120,
      h: 60,
      fill: '#dae8fc',
      stroke: '#6c8ebf',
      fontColor: '#333333',
      strokeWidth: 2,
      fontSize: 14,
    });
    expect(byId('oval')).toEqual({ id: 'oval', label: 'Oval', shape: 'ellipse', x: 240, y: 60, w: 80, h: 80, dash: 'dashed' });
    expect(byId('dec')).toMatchObject({ shape: 'diamond', x: 0, y: 200, dash: 'dotted' });
    expect(byId('note')).toMatchObject({ shape: 'text', label: 'Just text' });
  });

  it('turns a swimlane into a group with absolute child positions', () => {
    expect(g.groups).toEqual([{ id: 'lane', label: 'Lane', x: 500, y: 100, w: 200, h: 200 }]);
    expect(byId('c1')).toMatchObject({ shape: 'rect', parent: 'lane', x: 510, y: 130, w: 80, h: 40 });
    expect(byId('c2')).toMatchObject({ shape: 'cylinder', parent: 'lane', x: 510, y: 190 });
  });

  it('uses the id and label of a wrapping object', () => {
    expect(byId('obj')).toMatchObject({ shape: 'hexagon', label: 'Wrapped & tagged', x: 800, y: 0 });
  });

  it('reads edges', () => {
    const e = Object.fromEntries(g.edges.map((x) => [x.id, x]));
    expect(e.e1).toEqual({
      id: 'e1',
      from: 'box',
      to: 'oval',
      label: 'goes to',
      start: 'dot',
      end: 'open',
      dash: 'dashed',
      width: 3,
      stroke: '#ff0000',
      route: 'elbow',
    });
    expect(e.e2).toEqual({ id: 'e2', from: 'dec', toPoint: { x: 300, y: 400 }, start: 'none', end: 'none', route: 'straight' });
    expect(e.e3).toMatchObject({ from: 'obj', to: 'c2', end: 'arrow', start: 'none', route: 'curved' });
    // a label from a child cell; a group as one end
    expect(e.e4).toMatchObject({ from: 'c1', to: 'lane', label: 'Label\ncell', route: 'elbow' });
    // no target and no point: dropped
    expect(e.e5).toBeUndefined();
    expect(g.edges).toHaveLength(4);
    expect(g.nodes.some((n) => n.id === 'e4-label')).toBe(false);
  });

  it('uses the source point when there is no source cell', () => {
    const x = parseDrawio(`<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>
      <mxCell id="a" vertex="1" parent="1"><mxGeometry x="0" y="0" width="10" height="10" as="geometry"/></mxCell>
      <mxCell id="e" edge="1" parent="1" target="a" source="missing"><mxGeometry relative="1" as="geometry"><mxPoint x="-50" y="7" as="sourcePoint"/></mxGeometry></mxCell>
    </root></mxGraphModel>`);
    expect(x.edges).toEqual([{ id: 'e', fromPoint: { x: -50, y: 7 }, to: 'a', start: 'none', end: 'arrow', route: 'straight' }]);
  });

  it('reads a compressed diagram the same way', () => {
    const packed = `<mxfile><diagram id="x" name="Page-1">${compress(MODEL)}</diagram></mxfile>`;
    expect(parseDrawio(packed)).toEqual(g);
  });

  it('reads a compressed diagram with non-ASCII labels', () => {
    const model = `<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="n" value="Ünïcødé — 日本" vertex="1" parent="1"><mxGeometry width="10" height="10" as="geometry"/></mxCell></root></mxGraphModel>`;
    const x = parseDrawio(`<mxfile><diagram>\n  ${compress(model)}\n</diagram></mxfile>`);
    expect(x.nodes[0].label).toBe('Ünïcødé — 日本');
  });

  it('reads a bare mxGraphModel and nested containers', () => {
    const x = parseDrawio(`<?xml version="1.0"?><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>
      <mxCell id="outer" value="" style="group" vertex="1" parent="1"><mxGeometry x="100" y="100" width="300" height="300" as="geometry"/></mxCell>
      <mxCell id="inner" value="Inner" style="rounded=0;" vertex="1" parent="outer"><mxGeometry x="10" y="20" width="200" height="200" as="geometry"/></mxCell>
      <mxCell id="leaf" value="Leaf" style="ellipse;shape=cloud;" vertex="1" parent="inner"><mxGeometry x="5" y="5" width="50" height="40" as="geometry"/></mxCell>
      <mxCell id="star" value="" style="verticalLabelPosition=bottom;shape=mxgraph.basic.star;" vertex="1" parent="1"><mxGeometry x="1" y="2" width="3" height="4" as="geometry"/></mxCell>
    </root></mxGraphModel>`);
    expect(x.groups).toEqual([
      { id: 'outer', label: '', x: 100, y: 100, w: 300, h: 300 },
      { id: 'inner', label: 'Inner', parent: 'outer', x: 110, y: 120, w: 200, h: 200 },
    ]);
    expect(x.nodes).toEqual([
      { id: 'leaf', label: 'Leaf', shape: 'cloud', parent: 'inner', x: 115, y: 125, w: 50, h: 40 },
      { id: 'star', label: '', shape: 'star', x: 1, y: 2, w: 3, h: 4 },
    ]);
  });

  it('strips HTML labels and decodes entities', () => {
    const cell = (id: string, value: string, style = 'html=1;') =>
      `<mxCell id="${id}" value="${value}" style="${style}" vertex="1" parent="1"><mxGeometry width="1" height="1" as="geometry"/></mxCell>`;
    const x = parseDrawio(`<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>
      ${cell('a', 'Line 1&lt;div&gt;Line 2&lt;/div&gt;&lt;div&gt;&lt;b&gt;Line&amp;nbsp;3&lt;/b&gt;&lt;/div&gt;')}
      ${cell('b', '&lt;p style=&quot;margin:0&quot;&gt;Para&lt;/p&gt;&lt;p&gt;Two&amp;amp;more&lt;/p&gt;')}
      ${cell('c', 'Not &lt;html&gt;&#10;second&#xA;third', '')}
      ${cell('d', 'Fish &amp;amp; chips &#8364;5 &apos;ok&apos;')}
      ${cell('e', 'Free', 'strokeColor=none;fillColor=none;')}
    </root></mxGraphModel>`);
    expect(x.nodes.map((n) => n.label)).toEqual(['Line 1\nLine 2\nLine 3', 'Para\nTwo&more', 'Not <html>\nsecond\nthird', 'Fish & chips €5 \'ok\'', 'Free']);
    expect(x.nodes[4].shape).toBe('text');
  });

  it('reads an editable .drawio.svg', () => {
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" content="${FILE.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')}"><g/></svg>`;
    expect(parseDrawio(svg)).toEqual(g);
  });

  it('throws a friendly error on other input', () => {
    expect(() => parseDrawio('not xml at all')).toThrow(/Not a draw.io diagram/);
    expect(() => parseDrawio('<html><body>hi</body></html>')).toThrow(/Not a draw.io diagram/);
    expect(() => parseDrawio('<mxfile><diagram>@@@not base64@@@</diagram></mxfile>')).toThrow(/compressed draw.io/);
    expect(() => parseDrawio('')).toThrow(Error);
  });
});
