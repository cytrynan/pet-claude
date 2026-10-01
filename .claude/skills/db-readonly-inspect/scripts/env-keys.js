#!/usr/bin/env node
// .env の「鍵名」と接続先(ホスト/ポート/DB名)だけを表示する。値は一切出さない。
// 使い方: node env-keys.js <envfile> [--url-key KEY]
"use strict";
const { loadEnvFile, pickUrl, describeUrl, parseArgs } = require("./lib");

const args = parseArgs(process.argv.slice(2));
const file = args._[0] || args.env;
if (!file) {
  console.error("使い方: node env-keys.js <envfile> [--url-key KEY]");
  process.exit(2);
}
const env = loadEnvFile(file);
const keys = Object.keys(env);
console.log(`keys (${keys.length}):`);
for (const k of keys) {
  const v = env[k];
  const kind = /^postgres(ql)?:\/\//.test(v) ? "postgres-url" : /^(mysql|mongodb|redis|https?):\/\//.test(v) ? "other-url" : v === "" ? "empty" : `value(${v.length} chars)`;
  console.log(`  ${k.padEnd(28)} ${kind}`);
}
console.log("\npostgres url candidates (no secrets):");
for (const k of keys) {
  if (!/^postgres(ql)?:\/\//.test(env[k])) continue;
  try {
    const d = describeUrl(env[k]);
    console.log(`  ${k.padEnd(28)} ${d.host}:${d.port}/${d.database} ${JSON.stringify(d.params)}`);
  } catch (e) {
    console.log(`  ${k.padEnd(28)} (URL として解釈できません)`);
  }
}
try {
  const { key } = pickUrl(env, args["url-key"]);
  console.log(`\nselected: ${key}`);
} catch (e) {
  console.log(`\nselected: (none) ${e.message}`);
  process.exit(1);
}
