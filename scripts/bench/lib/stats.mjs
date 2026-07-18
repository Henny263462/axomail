/**
 * Small, dependency-free numeric helpers shared by the bench scripts.
 * All functions operate on plain arrays of numbers (milliseconds).
 */

/** Returns the smallest value, or `null` for an empty array. */
export function min(samples) {
  if (samples.length === 0) return null;
  return samples.reduce((a, b) => (b < a ? b : a), samples[0]);
}

/** Returns the largest value, or `null` for an empty array. */
export function max(samples) {
  if (samples.length === 0) return null;
  return samples.reduce((a, b) => (b > a ? b : a), samples[0]);
}

/** Returns the median value (average of the two middle values for even
 * lengths), or `null` for an empty array. */
export function median(samples) {
  if (samples.length === 0) return null;
  const sorted = [...samples].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  if (sorted.length % 2 === 0) {
    return (sorted[mid - 1] + sorted[mid]) / 2;
  }
  return sorted[mid];
}

/** Rounds to a fixed number of decimals, returned as a Number (not a
 * string), so JSON output stays compact. `null` passes through unchanged. */
export function round(value, decimals = 2) {
  if (value === null || value === undefined) return null;
  const factor = 10 ** decimals;
  return Math.round(value * factor) / factor;
}

/** Summarizes a list of numeric samples into {samples, min, median,
 * max}, each rounded to 2 decimals. Unit-agnostic on purpose — callers
 * name the unit in the surrounding JSON key (e.g. `workingSetKb`,
 * `cold`) rather than baking it into these field names. */
export function summarize(samples) {
  return {
    samples: samples.map((s) => round(s)),
    min: round(min(samples)),
    median: round(median(samples)),
    max: round(max(samples)),
  };
}
