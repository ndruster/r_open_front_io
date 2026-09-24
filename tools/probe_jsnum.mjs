const rows = [-1.5, -2.5, 1.9, -1, 4294967296, 4294967295, 70000.5, 65536, -65537, 2147483648, -2147483649, NaN, Infinity, -Infinity, -0, 0.5, -0.5, 9007199254740993];
for (const v of rows) {
  const o = {
    in: String(v),
    i32_or: v | 0,
    u32_shift: v >>> 0,
    fround: Math.fround(v),
    floor: Math.floor(v),
    trunc: Math.trunc(v),
  };
  console.log(JSON.stringify(o));
}
const a = new Uint16Array(6);
a[0] = -1; a[1] = 65536; a[2] = 70000.5; a[3] = -65537; a[4] = 1.9; a[5] = Infinity;
console.log("Uint16Array:", JSON.stringify(Array.from(a)));
const b = new Int32Array(4);
b[0] = -1.5; b[1] = -2.5; b[2] = 1.9; b[3] = 4294967295;
console.log("Int32Array:", JSON.stringify(Array.from(b)));
const c = new Uint32Array(4);
c[0] = -1.5; c[1] = -2.5; c[2] = 1.9; c[3] = 4294967296;
console.log("Uint32Array:", JSON.stringify(Array.from(c)));
const f = new Float32Array(6);
f[0] = 1 + 2 ** -24; f[1] = 1 + 2 ** -23; f[2] = -1.5; f[3] = 340282366920938463463374607431768211456; f[4] = NaN; f[5] = -0;
console.log("Float32Array:", JSON.stringify(Array.from(f)), "isNegZero:", Object.is(f[5], -0));
