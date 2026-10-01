#!/usr/bin/env node
// スキル自身の故障注入テスト。**ローカル/使い捨ての DB に対してだけ実行する**。
//
//   node selftest.js --env <local .env> [--url-key DATABASE_URL] [--client-root DIR]
//
// 確かめること:
//   A. lint が書き込み系を送信前に拒否し、読み取り系は通す(DB 不要)
//   B. 実 DB で SET TRANSACTION READ ONLY が効き、SHOW が on を返す
//   C. lint を外して書き込み(CREATE TEMP TABLE)を送っても、サーバー側が拒否する
//      (万一ガードが壊れていても一時テーブルは接続終了で消えるので、DB に痕跡は残らない)
//   D. 接続先が localhost 系でなければ C を実行しない(--i-know-this-is-a-test-db で解除)
"use strict";
const { loadEnvFile, pickUrl, describeUrl, lintReadOnlySql, runReadOnly, parseArgs } = require("./lib");

let failures = 0;
const ok = (name, cond, detail = "") => { console.log(`${cond ? "PASS" : "FAIL"}  ${name}${detail ? "  — " + detail : ""}`); if (!cond) failures++; };

// ---- A. lint ----------------------------------------------------------------
const mustReject = [
  "update t set a=1",
  "delete from t",
  "insert into t values (1)",
  "with d as (delete from t returning *) select * from d",
  "select 1; drop table t",
  "select * into newtab from t",
  "create temp table x(a int)",
  "select nextval('seq')",
  "select pg_terminate_backend(1)",
  "explain analyze select * from t",
  "truncate t",
  "-- comment\nupdate t set a=1",
];
const mustAccept = [
  "select 1",
  "SELECT count(*) FROM \"Item\"",
  "with x as (select 1 as a) select * from x",
  "show transaction_read_only",
  "explain select * from t",
  "select * from t where kind='update' and note like '%delete%'",
  "select \"updatedAt\", \"createdAt\", \"deletedAt\" from t",
  "select count(*) filter (where status='created') from t",
  "select * from information_schema.tables where table_schema='public';",
];
for (const sql of mustReject) ok(`lint rejects: ${sql.replace(/\n/g, " ")}`, lintReadOnlySql(sql) !== null, lintReadOnlySql(sql) || "");
for (const sql of mustAccept) ok(`lint accepts: ${sql.replace(/\n/g, " ")}`, lintReadOnlySql(sql) === null, lintReadOnlySql(sql) || "");

(async () => {
  const args = parseArgs(process.argv.slice(2));
  if (!args.env) { console.log("\n(--env 未指定のため DB を使うテスト B/C はスキップ)"); process.exit(failures ? 1 : 0); }
  const env = loadEnvFile(args.env);
  const { key, url } = pickUrl(env, args["url-key"]);
  const d = describeUrl(url);
  const isLocal = /^(localhost|127\.0\.0\.1|\[?::1\]?|.*\.localhost|host\.docker\.internal)$/i.test(d.host);
  console.log(`\ntarget: ${d.host}:${d.port}/${d.database} (env ${key}) local=${isLocal}`);
  const opts = { clientRoot: args["client-root"] };

  // ---- B. 読み取り専用の証跡 ----------------------------------------------
  try {
    const r = await runReadOnly(url, [{ name: "one", sql: "select 1 as one" }], opts);
    ok("B. SHOW transaction_read_only = on", r.proof.transaction_read_only === "on", JSON.stringify(r.proof.transaction_read_only));
    ok("B. select 1 returns a row", Array.isArray(r.results.one) && Number(r.results.one[0].one) === 1);
  } catch (e) {
    ok("B. connect & run", false, String(e.message || e).split("\n")[0]);
  }

  // ---- C. サーバー側ガードの故障注入(lint を外す) ---------------------------
  if (!isLocal && !args["i-know-this-is-a-test-db"]) {
    console.log("SKIP  C. 接続先が localhost 系ではないため、書き込み試行は行いません(--i-know-this-is-a-test-db で解除)");
  } else {
    try {
      const r = await runReadOnly(url, [{ name: "probe", sql: "create temp table _ro_probe_(x int)" }], { ...opts, lint: false });
      ok("C. server rejects CREATE TEMP TABLE in read-only txn", false, "エラーにならず実行された: " + JSON.stringify(r.results.probe));
    } catch (e) {
      const msg = String(e.message || e);
      ok("C. server rejects CREATE TEMP TABLE in read-only txn", /read-only transaction/i.test(msg), msg.split("\n").find((l) => /read-only/i.test(l)) || msg.split("\n")[0]);
    }
    // lint 有効なら送信前に止まる(ネットワークに出ない)
    try {
      await runReadOnly(url, [{ name: "probe", sql: "create temp table _ro_probe_(x int)" }], opts);
      ok("C. lint blocks the same statement before sending", false);
    } catch (e) {
      ok("C. lint blocks the same statement before sending", /^\[lint\]/.test(String(e.message)), String(e.message).split("\n")[0]);
    }
  }
  console.log(`\n${failures === 0 ? "ALL PASS" : failures + " FAILED"}`);
  process.exit(failures ? 1 : 0);
})();
