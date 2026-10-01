#!/usr/bin/env node
// ro-query.js / inspect.js が保存した JSON を読みやすく表示する。
//   node print.js <out.json> [--limit 40] [--only name1,name2]
"use strict";
const fs = require("fs");
const { printResults, parseArgs } = require("./lib");

const args = parseArgs(process.argv.slice(2));
const file = args._[0];
if (!file) { console.error("使い方: node print.js <out.json> [--limit N] [--only a,b]"); process.exit(2); }
const doc = JSON.parse(fs.readFileSync(file, "utf8"));
if (doc.target) console.log(`target: ${doc.target.host}:${doc.target.port}/${doc.target.database} (env ${doc.target.env_key})  generated_at=${doc.generated_at}`);
if (doc.proof) console.log(`proof: ${JSON.stringify(doc.proof)}`);
let results = doc.results || {};
if (!doc.results && doc.tables) {
  // inspect.js の出力
  results = { tables: doc.tables, foreign_keys: doc.foreign_keys, primary_keys: doc.primary_keys, columns: doc.columns };
}
if (args.only) {
  const keep = String(args.only).split(",").map((s) => s.trim());
  results = Object.fromEntries(Object.entries(results).filter(([k]) => keep.includes(k)));
}
printResults(results, args.limit ? Number(args.limit) : 40);
