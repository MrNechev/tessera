// Viewport-based mounting. Media is created when its element comes within `near` of the
// scroll viewport and destroyed again once it is beyond `far` (hysteresis prevents
// thrashing). A third observer tracks real visibility for play/pause.
//
// One IntersectionObserver per (root, margin) is shared by every element.

type Cb = (v: boolean) => void;

class SharedObserver {
  private io: IntersectionObserver;
  private cbs = new Map<Element, Cb>();
  constructor(root: Element | null, margin: string, threshold = 0) {
    this.io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) this.cbs.get(e.target)?.(e.isIntersecting);
      },
      { root, rootMargin: margin, threshold },
    );
  }
  observe(el: Element, cb: Cb) {
    this.cbs.set(el, cb);
    this.io.observe(el);
  }
  unobserve(el: Element) {
    this.cbs.delete(el);
    this.io.unobserve(el);
  }
}

const observers = new Map<string, SharedObserver>();
const rootIds = new WeakMap<Element, number>();
let nextRoot = 1;

function shared(root: Element | null, margin: string, threshold = 0) {
  let rid = 0;
  if (root) {
    rid = rootIds.get(root) ?? 0;
    if (!rid) rootIds.set(root, (rid = nextRoot++));
  }
  const k = `${rid}|${margin}|${threshold}`;
  let o = observers.get(k);
  if (!o) observers.set(k, (o = new SharedObserver(root, margin, threshold)));
  return o;
}

export interface NearOpts {
  near?: string;
  far?: string;
  onNear: Cb;
  onVisible?: Cb;
  /** false: do nothing (the parent decides when to mount, e.g. on the canvas) */
  enabled?: boolean;
}

/** Svelte action: reports near/far (mount/unmount) and visible (play/pause). */
export function viewport(node: Element, opts: NearOpts) {
  if (opts.enabled === false) return {};
  // measure against the nearest scroll container marked with data-scroll-root
  const root = node.closest('[data-scroll-root]');
  const nearO = shared(root, opts.near ?? '120% 0px 120% 0px');
  const farO = shared(root, opts.far ?? '350% 0px 350% 0px');
  const visO = shared(root, '0px', 0.15);
  let o = opts;
  let isNear = false;
  nearO.observe(node, (v) => {
    if (v && !isNear) {
      isNear = true;
      o.onNear(true);
    }
  });
  farO.observe(node, (v) => {
    if (!v && isNear) {
      isNear = false;
      o.onNear(false);
    }
  });
  visO.observe(node, (v) => o.onVisible?.(v));
  return {
    update(n: NearOpts) {
      o = n;
    },
    destroy() {
      nearO.unobserve(node);
      farO.unobserve(node);
      visO.unobserve(node);
    },
  };
}
