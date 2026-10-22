// The name of a vault's directory in the core's disk cache
// (`<data>/cache/pages/<name>`): `notes-core::cache::DiskCache::new` hashes the
// vault's location (its canonical path) with `StableHasher` (SipHash-1-3 with
// zero keys over the length as a little-endian u64 and the bytes). The e2e copy
// a warmed data directory to another path and move its cache under the new
// name, so the copy starts warm. `server.ts` checks the scheme on the template
// before it relies on it.

const MASK = (1n << 64n) - 1n;
const rotl = (x: bigint, b: bigint): bigint => ((x << b) | (x >> (64n - b))) & MASK;

function round(v: bigint[]): void {
  let [v0, v1, v2, v3] = v as [bigint, bigint, bigint, bigint];
  v0 = (v0 + v1) & MASK;
  v1 = rotl(v1, 13n) ^ v0;
  v0 = rotl(v0, 32n);
  v2 = (v2 + v3) & MASK;
  v3 = rotl(v3, 16n) ^ v2;
  v0 = (v0 + v3) & MASK;
  v3 = rotl(v3, 21n) ^ v0;
  v2 = (v2 + v1) & MASK;
  v1 = rotl(v1, 17n) ^ v2;
  v2 = rotl(v2, 32n);
  v.splice(0, 4, v0, v1, v2, v3);
}

/** SipHash-1-3 with the keys 0, 0 (`siphasher::sip::SipHasher13::new`), as 16 hex digits. */
export function sipHash13(data: Uint8Array): string {
  const v = [0x736f6d6570736575n, 0x646f72616e646f6dn, 0x6c7967656e657261n, 0x7465646279746573n];
  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  const whole = data.length - (data.length % 8);
  const compress = (m: bigint) => {
    v[3]! ^= m;
    round(v);
    v[0]! ^= m;
  };
  for (let i = 0; i < whole; i += 8) compress(view.getBigUint64(i, true));
  let last = BigInt(data.length & 0xff) << 56n;
  for (let i = whole; i < data.length; i++) last |= BigInt(data[i]!) << BigInt(8 * (i - whole));
  compress(last);
  v[2]! ^= 0xffn;
  for (let i = 0; i < 3; i++) round(v);
  return ((v[0]! ^ v[1]! ^ v[2]! ^ v[3]!) & MASK).toString(16).padStart(16, "0");
}

/** The cache directory name of the vault at `location` (`StableHasher::new().str(location).hex()`). */
export function vaultCacheKey(location: string): string {
  const bytes = new TextEncoder().encode(location);
  const data = new Uint8Array(8 + bytes.length);
  new DataView(data.buffer).setBigUint64(0, BigInt(bytes.length), true);
  data.set(bytes, 8);
  return sipHash13(data);
}
