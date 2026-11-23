// Message channel shared by a presentation and paired phones (server-sent events + POST).
export class RemoteChannel {
  private es: EventSource | null = null;
  private closed = false;
  constructor(
    private doc: string,
    private onmsg: (m: any) => void,
  ) {
    this.open();
  }
  private open() {
    if (this.closed) return;
    this.es = new EventSource(`/api/remote/${this.doc}/stream`, { withCredentials: true });
    this.es.onmessage = (e) => {
      try {
        this.onmsg(JSON.parse(e.data));
      } catch {}
    };
    this.es.onerror = () => {
      // EventSource reconnects by itself; a 401 closes it for good
      if (this.es?.readyState === EventSource.CLOSED) setTimeout(() => this.open(), 3000);
    };
  }
  send(msg: Record<string, unknown>) {
    return fetch(`/api/remote/${this.doc}/send`, {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(msg),
    }).catch(() => {});
  }
  close() {
    this.closed = true;
    this.es?.close();
  }
}

export async function pair(doc: string): Promise<{ token: string; urls: string[] }> {
  const r = await fetch(`/api/remote/${doc}/pair`, { method: 'POST', credentials: 'same-origin' });
  if (!r.ok) throw new Error('pairing failed');
  return r.json();
}
