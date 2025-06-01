// Polyfill Array.prototype.toSorted since Vue tsconfig by default only supports ES2020 at the time
// of writing.
export function sort<T>(array: Array<T>): Array<T> {
  array.sort();
  return array;
}
