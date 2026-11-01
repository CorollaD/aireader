/**
 * PP-OCRv6 small: tiny is three times faster, but on poor scans its error rate is several times
 * higher (26% against 7% on simulated photocopies), and poor scans are what this is for.
 */
export const OCR_MODELS = { detection: "PP-OCRv6_small_det", recognition: "PP-OCRv6_small_rec" };
