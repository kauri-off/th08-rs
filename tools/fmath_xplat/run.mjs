// wasm side of the fmath cross-target check: same output format as the native binary.
import { readFileSync } from "node:fs";
const { instance } = await WebAssembly.instantiate(readFileSync(process.argv[2]), {});
const { digest } = instance.exports;
for (let k = 0; k < 10; k++)
  for (let c = 0; c < 16; c++)
    console.log(`${k} ${c} ${(BigInt.asUintN(64, digest(k, c))).toString(16).padStart(16, "0")}`);
