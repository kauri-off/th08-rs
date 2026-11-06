// wasm side of the fmath cross-target check: same output format as the native binary.
import { readFileSync } from "node:fs";
const { instance } = await WebAssembly.instantiate(readFileSync(process.argv[2]), {});
const { digest, digest_raw, nan_probe } = instance.exports;
const hex = (v, n) => BigInt.asUintN(n, BigInt(v)).toString(16).padStart(n / 4, "0");
for (let k = 0; k < 10; k++) for (let c = 0; c < 16; c++) console.log(`${k} ${c} ${hex(digest(k, c), 64)}`);
for (let k = 0; k < 10; k++) for (let c = 0; c < 16; c++) console.log(`raw ${k} ${c} ${hex(digest_raw(k, c), 64)}`);
for (let i = 0; i < 8; i++) console.log(`nan ${i} ${hex(nan_probe(i), 32)}`);
