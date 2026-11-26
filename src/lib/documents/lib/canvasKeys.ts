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
    else if (ed.help) ed.help = false;
    else if (ed.stickerPicker) ed.stickerPicker = false;
    else if (ed.styleMenu) ed.styleMenu = false;
    else if (ed.drawer) ed.drawer = null;
    else if (ed.tool !== 'select') ed.tool = 'select';
    else ed.select([]);
    return;
  }
  const single = ed.single;
  if (k === 'Enter' && single) {
    e.preventDefault();
    if (ed.secMap.has(single)) ed.openDrawer(single);
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
  const t = TOOLS.find((x) => x.key === k.toUpperCase());
  if (t && !e.shiftKey) ed.tool = t.id;
}

export function canvasKeyUp(ed: CanvasEditor, e: KeyboardEvent) {
  if (e.key === ' ') ed.spaceDown = false;
}
