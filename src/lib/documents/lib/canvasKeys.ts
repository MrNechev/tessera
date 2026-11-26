// Keyboard shortcuts of the canvas editor (the help panel lists them).
import { TOOLS, typing, type CanvasEditor } from './canvasEditor.svelte';

const GRID_STEP = 8;

export function canvasKeyDown(ed: CanvasEditor, e: KeyboardEvent) {
  if (typing(e.target)) return;
  const mod = e.ctrlKey || e.metaKey;
  const k = e.key;
  if (k === ' ') {
    e.preventDefault();
    ed.spaceDown = true;
    return;
  }
  if (mod && (k === 'z' || k === 'Z')) {
    e.preventDefault();
    e.shiftKey ? ed.redo() : ed.undo();
    return;
  }
  if (mod && (k === 'y' || k === 'Y')) {
    e.preventDefault();
    ed.redo();
    return;
  }
  // copy / paste style (before plain Ctrl+C / Ctrl+V)
  if (mod && e.altKey && (k === 'c' || k === 'C' || e.code === 'KeyC')) {
    e.preventDefault();
    ed.copyStyle();
    return;
  }
  if (mod && e.altKey && (k === 'v' || k === 'V' || e.code === 'KeyV')) {
    e.preventDefault();
    ed.pasteStyle();
    return;
  }
  if (mod && (k === 'd' || k === 'D') && !e.shiftKey) {
    e.preventDefault();
    ed.duplicateSelection();
    return;
  }
  if (mod && (k === 'a' || k === 'A')) {
    e.preventDefault();
    ed.select(ed.allIds().filter((id) => !ed.isLocked(id)));
    return;
  }
  if (mod && (k === 'c' || k === 'C') && ed.selected.size) {
    e.preventDefault();
    ed.copySelection();
    return;
  }
  if (mod && (k === 'x' || k === 'X') && ed.selected.size) {
    e.preventDefault();
    ed.copySelection(true);
    return;
  }
  if (mod && (k === ']' || k === '[') && ed.selected.size) {
    e.preventDefault();
    ed.reorder(k === ']');
    return;
  }
  if (mod && (k === 'g' || k === 'G')) {
    e.preventDefault();
    e.shiftKey ? ed.ungroupSelection() : ed.groupSelection();
    return;
  }
  if (mod && e.shiftKey && (k === 'l' || k === 'L')) {
    e.preventDefault();
    ed.toggleLock();
    return;
  }
  if (mod && k === '0') {
    e.preventDefault();
    ed.zoom100();
    return;
  }
  if (mod || e.altKey) return;
  if (k === 'Delete' || k === 'Backspace') {
    e.preventDefault();
    ed.deleteSelection();
    return;
  }
  if (k === 'Escape') {
    if (ed.editingCard) ed.editingCard = null;
    else if (ed.mermaidText !== null) ed.mermaidText = null;
    else if (ed.help) ed.help = false;
    else if (ed.shapePicker || ed.diagramMenu) ed.shapePicker = ed.diagramMenu = false;
    else if (ed.stickerPicker) ed.stickerPicker = false;
    else if (ed.styleMenu) ed.styleMenu = false;
    else if (ed.stylePanel) ed.stylePanel = false;
    else if (ed.drawer) ed.drawer = null;
    else if (ed.tool !== 'select') ed.tool = 'select';
    else ed.select([]);
    return;
  }
  const single = ed.single;
  // mind-map flow: Tab adds a connected child, Enter a sibling below a child
  if (k === 'Tab' && single && !e.shiftKey) {
    const it = ed.itemMap.get(single);
    if (it && (it.type === 'rect' || it.type === 'ellipse' || it.type === 'note')) {
      e.preventDefault();
      ed.addChild();
    }
    return;
  }
  if (k === 'F2' && single) {
    e.preventDefault();
    ed.startEdit(single);
    return;
  }
  if (k === 'Enter' && single) {
    e.preventDefault();
    if (ed.secMap.has(single)) ed.openDrawer(single);
    else if (!e.shiftKey && ed.addSibling()) return;
    else ed.startEdit(single);
    return;
  }
  if (k.startsWith('Arrow') && ed.selected.size) {
    e.preventDefault();
    const n = e.shiftKey ? 10 * GRID_STEP : GRID_STEP;
    ed.nudge(k === 'ArrowLeft' ? -n : k === 'ArrowRight' ? n : 0, k === 'ArrowUp' ? -n : k === 'ArrowDown' ? n : 0);
    return;
  }
  if (e.shiftKey && (k === '!' || k === '1')) {
    ed.fitAll();
    return;
  }
  if (e.shiftKey && (k === '@' || k === '2')) {
    ed.zoomToSelection();
    return;
  }
  if (k === '+' || k === '=') return ed.zoomBy(1.25);
  if (k === '-' || k === '_') return ed.zoomBy(0.8);
  if (k === '?') {
    ed.help = !ed.help;
    return;
  }
  if ((k === 's' || k === 'S') && !e.shiftKey) {
    ed.stickerPicker = !ed.stickerPicker;
    return;
  }
  if (e.shiftKey) return;
  const u = k.toUpperCase();
  // shapes: R rectangle, O ellipse, D the last shape picked in the shape picker
  if (u === 'R' || u === 'O') return ed.setShape(u === 'R' ? 'rect' : 'ellipse');
  if (u === 'D') return ed.setShape(ed.lastPicked);
  if (u === 'E' && ed.tool === 'eraser') return ed.setEraser(ed.eraserMode === 'stroke' ? 'split' : 'stroke');
  const t = TOOLS.find((x) => x.key === u);
  if (t) ed.tool = t.id;
}

export function canvasKeyUp(ed: CanvasEditor, e: KeyboardEvent) {
  if (e.key === ' ') ed.spaceDown = false;
}
