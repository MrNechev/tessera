// Tiny hash router: #/ (documents), #/doc/<id>[/canvas], #/present/<id>[?s=n&canvas=1],
// #/presenter/<id>[?canvas=1], #/system

export type Route =
  | { name: 'home' }
  | { name: 'doc'; id: string; view: 'doc' | 'canvas'; section?: string }
  | { name: 'present'; id: string; start: number; canvas: boolean }
  | { name: 'presenter'; id: string; canvas: boolean }
  | { name: 'system' }
  | { name: 'remote'; id: string }
  | { name: 'print'; id: string; mode: 'doc' | 'slides' | 'canvas' };

function parse(): Route {
  const h = location.hash.replace(/^#/, '') || '/';
  const [path, query] = h.split('?');
  const q = new URLSearchParams(query || '');
  const parts = path.split('/').filter(Boolean);
  if (parts[0] === 'doc' && parts[1])
    return { name: 'doc', id: parts[1], view: parts[2] === 'canvas' ? 'canvas' : 'doc', section: q.get('s') || undefined };
  if (parts[0] === 'present' && parts[1])
    return { name: 'present', id: parts[1], start: Number(q.get('s') || 0), canvas: q.get('canvas') === '1' };
  if (parts[0] === 'presenter' && parts[1]) return { name: 'presenter', id: parts[1], canvas: q.get('canvas') === '1' };
  if (parts[0] === 'system') return { name: 'system' };
  if (parts[0] === 'remote' && parts[1]) return { name: 'remote', id: parts[1] };
  if (parts[0] === 'print' && parts[1]) {
    const m = q.get('mode');
    return { name: 'print', id: parts[1], mode: m === 'slides' || m === 'canvas' ? m : 'doc' };
  }
  return { name: 'home' };
}

class Router {
  route = $state<Route>(parse());
  constructor() {
    addEventListener('hashchange', () => (this.route = parse()));
  }
  go(hash: string) {
    location.hash = hash;
  }
}

export const router = new Router();
