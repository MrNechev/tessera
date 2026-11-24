/**
 * Background removal off the main thread: loads onnxruntime-web's plain
 * WebAssembly backend (no WebGPU/WebGL, so Chromium and WebKitGTK behave the
 * same) and the U²-Netp model, then answers mask requests. Everything stays
 * on this machine — the model is served by Tessera itself.
 */
import * as ort from 'onnxruntime-web/wasm';
// Vite copies the runtime into dist/assets (hashed, cached forever) — in the
// Docker image and in the desktop binary's embedded assets alike.
import wasmUrl from 'onnxruntime-web/ort-wasm-simd-threaded.wasm?url';
import { predictMask } from './model';

export type WorkerRequest = { type: 'segment'; id: number; modelUrl: string; rgba: Uint8ClampedArray };
export type WorkerReply =
  | { type: 'progress'; id: number; stage: string; fraction: number }
  | { type: 'mask'; id: number; mask: Float32Array }
  | { type: 'error'; id: number; message: string };

const post = (m: WorkerReply, transfer: Transferable[] = []) => (self as unknown as Worker).postMessage(m, transfer);

let session: Promise<ort.InferenceSession> | null = null;

async function download(url: string, onProgress: (f: number) => void): Promise<Uint8Array> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`Could not load the cut-out model (${res.status})`);
  // A missing file comes back as the app's index.html (SPA fallback).
  if ((res.headers.get('content-type') ?? '').includes('text/html')) throw new Error('The cut-out model is missing from this installation');
  const total = Number(res.headers.get('content-length')) || 0;
  if (!res.body || !total) return new Uint8Array(await res.arrayBuffer());
  const reader = res.body.getReader();
  const chunks: Uint8Array[] = [];
  let got = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    got += value.length;
    onProgress(Math.min(1, got / total));
  }
  const buf = new Uint8Array(got);
  let at = 0;
  for (const c of chunks) {
    buf.set(c, at);
    at += c.length;
  }
  return buf;
}

function load(modelUrl: string, id: number): Promise<ort.InferenceSession> {
  if (!session) {
    session = (async () => {
      ort.env.wasm.wasmPaths = { wasm: wasmUrl };
      // Threads need SharedArrayBuffer, i.e. a cross-origin isolated page;
      // Tessera's pages aren't, so this is usually 1 (and that's fine: ~1 s).
      ort.env.wasm.numThreads = self.crossOriginIsolated ? Math.min(4, navigator.hardwareConcurrency || 1) : 1;
      // We already are the worker; don't let ort spawn another one.
      ort.env.wasm.proxy = false;
      const model = await download(modelUrl, (f) => post({ type: 'progress', id, stage: 'Loading the cut-out model', fraction: 0.7 * f }));
      post({ type: 'progress', id, stage: 'Starting the engine', fraction: 0.75 });
      return ort.InferenceSession.create(model, { executionProviders: ['wasm'], graphOptimizationLevel: 'all' });
    })();
    // A failed load (offline dev server, old browser) may be retried later.
    session.catch(() => (session = null));
  }
  return session;
}

self.onmessage = async (e: MessageEvent<WorkerRequest>) => {
  const req = e.data;
  if (req.type !== 'segment') return;
  try {
    const s = await load(req.modelUrl, req.id);
    post({ type: 'progress', id: req.id, stage: 'Finding the subject', fraction: 0.8 });
    const mask = await predictMask(ort, s, req.rgba);
    post({ type: 'mask', id: req.id, mask }, [mask.buffer]);
  } catch (err) {
    let message = err instanceof Error ? err.message : String(err);
    if (/simd|compile|wasm/i.test(message) && !/model/i.test(message)) message = `This browser can't run the cut-out engine (WebAssembly SIMD is required): ${message}`;
    post({ type: 'error', id: req.id, message });
  }
};
