/**
 * U²-Netp (salient object detection, Apache-2.0; public/models/) — input
 * preparation and inference. Written against the small slice of the
 * onnxruntime API it needs, so the Web Worker (onnxruntime-web/wasm) and the
 * Node evaluation test (onnxruntime-web's Node build) share it.
 */
import type { InferenceSession, Tensor } from 'onnxruntime-web';
import { normalizeMask } from './mask';

/** The model's fixed input size. */
export const MODEL_SIZE = 320;
export const MODEL_URL = '/models/u2netp.onnx';

/** What we use from `onnxruntime-web`'s module object. */
export interface OrtLike {
  Tensor: new (type: 'float32', data: Float32Array, dims: number[]) => Tensor;
}

// ImageNet statistics, as the model was trained (and as rembg feeds it).
const MEAN = [0.485, 0.456, 0.406];
const STD = [0.229, 0.224, 0.225];

/**
 * RGBA pixels already scaled to MODEL_SIZE² → NCHW float input. Like the
 * reference implementation, values are divided by the image's brightest
 * channel value first, then standardised.
 */
export function toInput(rgba: Uint8ClampedArray | Uint8Array): Float32Array {
  const n = MODEL_SIZE * MODEL_SIZE;
  if (rgba.length !== n * 4) throw new Error(`expected ${MODEL_SIZE}×${MODEL_SIZE} RGBA`);
  let max = 1;
  for (let i = 0; i < n * 4; i++) if ((i & 3) !== 3 && rgba[i] > max) max = rgba[i];
  const out = new Float32Array(3 * n);
  for (let i = 0; i < n; i++) {
    for (let c = 0; c < 3; c++) out[c * n + i] = (rgba[i * 4 + c] / max - MEAN[c]) / STD[c];
  }
  return out;
}

/** Run the model on MODEL_SIZE² RGBA pixels; returns the normalised (0..1) mask, MODEL_SIZE². */
export async function predictMask(ort: OrtLike, session: InferenceSession, rgba: Uint8ClampedArray | Uint8Array): Promise<Float32Array> {
  const input = new ort.Tensor('float32', toInput(rgba), [1, 3, MODEL_SIZE, MODEL_SIZE]);
  // The first output (d0) is the fused, final prediction; d1…d6 are side outputs.
  const out = await session.run({ [session.inputNames[0]]: input }, [session.outputNames[0]]);
  const t = out[session.outputNames[0]];
  const data = new Float32Array(t.data as Float32Array);
  for (const v of Object.values(out)) v.dispose?.();
  input.dispose?.();
  return normalizeMask(data);
}
