#!/usr/bin/env node
// 任意の SELECT 群を読み取り専用トランザクションで実行し、JSON に保存する。
//
// 使い方:
//   node ro-query.js --env <envfile> --queries <q.json> [--out <out.json>]
//                    [--url-key DIRECT_URL] [--client-root <prisma のあるプロジェクト>]
//                    [--limit 40] [--quiet] [--timeout-ms 300000]
//
// q.json は [{"name":"...","sql":"select ..."}] または {"name":"select ...", ...}。
// 書き込み系は lint で送信前に拒否し、サーバー側も SET TRANSACTION READ ONLY で拒否する。
"use strict";
const fs = require("fs");
const path = require("path");
const { loadEnvFile, pickUrl, describeUrl, runReadOnly, printResults, parseArgs, jsonReplacer } = require("./lib");

function usage(code) {
  console.error("使い方: node ro-query.js --env <envfile> --queries <q.json> [--out <out.json>] [--url-key KEY] [--client-root DIR] [--limit N] [--quiet]");
  process.exit(code);
}

(async () => {
  const args = parseArgs(process.argv.slice(2));
  if (!args.env || !args.queries) usage(2);

  const env = loadEnvFile(args.env);
  const { key, url } = pickUrl(env, args["url-key"]);
  const target = { env_key: key, ...describeUrl(url) };

  let raw = JSON.parse(fs.readFileSync(args.queries, "utf8"));
  const queries = Array.isArray(raw) ? raw : Object.entries(raw).map(([name, sql]) => ({ name, sql }));
  for (const q of queries) if (!q.name || !q.sql) { console.error("queries の各要素は {name, sql} が必要です"); process.exit(2); }

  console.log(`target: ${target.host}:${target.port}/${target.database} (env ${key})`);
  console.log(`queries: ${queries.length}`);

  let out;
  try {
    out = await runReadOnly(url, queries, {
      clientRoot: args["client-root"],
      timeoutMs: args["timeout-ms"] ? Number(args["timeout-ms"]) : undefined,
      onQuery: args.quiet ? undefined : (name, ms) => process.stdout.write(`  ran ${name} (${ms} ms)\n`),
    });
  } catch (e) {
    console.error("ERROR:", String(e.message || e).split("\n").filter(Boolean).slice(0, 6).join("\n"));
    process.exit(1);
  }

  const errors = Object.entries(out.results).filter(([, v]) => v && !Array.isArray(v) && v.error);
  const doc = { generated_at: new Date().toISOString(), target, proof: out.proof, results: out.results };
  if (args.out) {
    fs.mkdirSync(path.dirname(path.resolve(args.out)), { recursive: true });
    fs.writeFileSync(args.out, JSON.stringify(doc, jsonReplacer, 2));
  }
  console.log(`\nproof: transaction_read_only=${out.proof.transaction_read_only} lint=${out.proof.lint} elapsed=${out.proof.elapsed_ms}ms`);
  console.log(`results: ${queries.length - errors.length} ok / ${errors.length} error${args.out ? ` -> ${args.out}` : ""}`);
  for (const [k, v] of errors) console.log(`  ERR ${k}: ${v.error}`);
  if (!args.quiet) printResults(out.results, args.limit ? Number(args.limit) : 40);
})();
