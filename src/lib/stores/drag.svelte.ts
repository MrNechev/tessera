/** In-app drag & drop of photos from the library tray onto the canvas. */

export interface DragSession {
  assetIds: string[];
  thumb: string;
  x: number;
  y: number;
}

class DragState {
  session = $state<DragSession | null>(null);

  start(assetIds: string[], thumb: string, x: number, y: number): void {
    this.session = { assetIds, thumb, x, y };
  }

  move(x: number, y: number): void {
    if (this.session) {
      this.session.x = x;
      this.session.y = y;
    }
  }

  end(): DragSession | null {
    const s = this.session;
    this.session = null;
    return s;
  }
}

export const drag = new DragState();
