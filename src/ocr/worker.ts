// Runs the recogniser. It lives in a worker of its own, not just to stay off the main thread:
// OpenCV needs eval, which the app's Content Security Policy rightly forbids in the page, and
// that policy comes with the HTML document, so it doesn't reach a worker. Importing the SDK from
// the page would break the page.

import { PaddleOCR } from "@paddleocr/paddleocr-js";
import cvModule from "@techstark/opencv-js";
import runtime from "onnxruntime-web/ort-wasm-simd-threaded.wasm?url";
import type { OcrLine } from "./layout";
import { OCR_MODELS } from "./models";

export type OcrRequest = { id: number; type: "init"; detection: string; recognition: string } | { id: number; type: "recognise"; image: Blob };

export interface OcrPage {
  width: number;
  height: number;
  lines: OcrLine[];
}

let ocr: Awaited<ReturnType<typeof PaddleOCR.create>> | null = null;

async function init(detection: string, recognition: string) {
  ocr = await PaddleOCR.create({
    textDetectionModelName: OCR_MODELS.detection,
    textRecognitionModelName: OCR_MODELS.recognition,
    textDetectionModelAsset: { url: detection },
    textRecognitionModelAsset: { url: recognition },
    // Typed as a string, but passed on as is to ONNX Runtime, which takes this form too.
    ortOptions: { backend: "wasm", wasmPaths: { wasm: runtime } as never, numThreads: 1, simd: true },
  });
}

/** The SDK only reads images through a DOM canvas, unless it is handed an OpenCV matrix. */
async function pixels(image: Blob): Promise<ImageData> {
  const bitmap = await createImageBitmap(image);
  const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) throw new Error("No 2D canvas in this worker");
  context.drawImage(bitmap, 0, 0);
  bitmap.close();
  return context.getImageData(0, 0, canvas.width, canvas.height);
}

async function recognise(image: Blob): Promise<OcrPage> {
  // Ready by now, as the SDK waited for it. Awaiting the module itself would never finish: it
  // has a `then` that resolves to itself.
  const cv = cvModule instanceof Promise ? await cvModule : cvModule;
  const data = await pixels(image);
  const mat = cv.matFromImageData(data);
  try {
    const [result] = await ocr!.predict(mat as never);
    return { width: data.width, height: data.height, lines: result.items.map(({ text, poly }) => ({ text, poly })) };
  } finally {
    mat.delete();
  }
}

self.onmessage = async ({ data }: MessageEvent<OcrRequest>) => {
  try {
    const result = data.type === "init" ? await init(data.detection, data.recognition) : await recognise(data.image);
    self.postMessage({ id: data.id, result });
  } catch (e) {
    self.postMessage({ id: data.id, error: e instanceof Error ? e.message : String(e) });
  }
};
