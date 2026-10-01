// db-readonly-inspect の共通ライブラリ。
//
// 方針:
//  - 接続情報(.env)の値は絶対に標準出力・ファイルに出さない(鍵名とホスト/ポート/DB名だけ)。
//  - 読み取り専用は「宣言」でなく「機構」で担保する:
//      ① クライアント側 lint(SELECT/WITH/SHOW/EXPLAIN 以外は送らない)
//      ② サーバー側 SET TRANSACTION READ ONLY(1文目)+ SHOW transaction_read_only の証跡
//  - DB クライアントは手元のどこかのプロジェクトに生成済みの Prisma Client を流用する
//    ($queryRawUnsafe はスキーマに依存しない)。追加インストール不要。
"use strict";

const fs = require("fs");
const path = require("path");

/** .env を自前で読む(dotenv を入れない)。値は返すだけで、呼び出し側も出力しないこと。 */
function loadEnvFile(file) {
  const out = {};
  const text = fs.readFileSync(file, "utf8").replace(/^\uFEFF/, "");
  for (const line of text.split(/\r?\n/)) {
    const m = line.match(/^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*?)\s*$/);
    if (!m) continue;
    let v = m[2];
    if ((v.startsWith('"') && v.endsWith('"')) || (v.startsWith("'") && v.endsWith("'"))) v = v.slice(1, -1);
    out[m[1]] = v;
  }
  return out;
}

/** 接続 URL の候補キー(優先順)。Supabase/Prisma 系は DIRECT_URL(セッションモード)を先に使う。 */
const URL_KEY_PRIORITY = ["DIRECT_URL", "DIRECT_DATABASE_URL", "DATABASE_URL", "POSTGRES_URL_NON_POOLING", "POSTGRES_URL", "POSTGRES_PRISMA_URL"];

function pickUrl(env, preferredKey) {
  if (preferredKey) {
    if (!env[preferredKey]) throw new Error(`env に ${preferredKey} がありません`);
    return { key: preferredKey, url: env[preferredKey] };
  }
  for (const k of URL_KEY_PRIORITY) if (env[k]) return { key: k, url: env[k] };
  const any = Object.keys(env).find((k) => /^postgres(ql)?:\/\//.test(env[k]));
  if (any) return { key: any, url: env[any] };
  throw new Error("postgres:// の接続 URL が env にありません");
}

/** URL に connect_timeout 等を足す(既にあれば触らない)。pooler の cold start で P1001 になるのを防ぐ。 */
function withDefaults(url, defaults = { connect_timeout: "30", pool_timeout: "30" }) {
  const u = new URL(url);
  for (const [k, v] of Object.entries(defaults)) if (!u.searchParams.has(k)) u.searchParams.set(k, v);
  return u.toString();
}

/** 秘密を含まない接続先の説明(ユーザー名・パスワードは出さない)。 */
function describeUrl(url) {
  const u = new URL(url);
  const params = {};
  for (const [k, v] of u.searchParams) params[k] = k.toLowerCase().includes("pass") ? "***" : v;
  return { host: u.hostname, port: u.port || "5432", database: decodeURIComponent(u.pathname.replace(/^\//, "")), params };
}

/** 生成済み Prisma Client を探す。clientRoot(既定 cwd)→ 親ディレクトリを順に辿る。 */
function resolvePrismaClient(clientRoot) {
  const tried = [];
  let dir = path.resolve(clientRoot || process.cwd());
  for (let i = 0; i < 6; i++) {
    try {
      const entry = require.resolve("@prisma/client", { paths: [dir] });
      const mod = require(entry);
      if (mod && mod.PrismaClient) {
        let version = "?";
        try { version = require(require.resolve("@prisma/client/package.json", { paths: [dir] })).version; } catch {}
        return { PrismaClient: mod.PrismaClient, from: dir, version };
      }
    } catch (e) {
      tried.push(dir);
    }
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  throw new Error(
    "生成済みの @prisma/client が見つかりません。--client-root に Prisma を使うプロジェクト(pnpm install 済み)を指定してください。探した場所: " + tried.join(", ")
  );
}

// ---------------------------------------------------------------------------
// SQL lint(クライアント側の網)
// ---------------------------------------------------------------------------

// 文頭は読み取り系に限定。文頭が SELECT/WITH なら DDL は埋め込めないので、
// 残る危険は「データ変更 CTE(WITH x AS (DELETE …))」「SELECT INTO」「副作用のある関数」の3つ。
const ALLOWED_HEAD = /^(select|with|show|explain|values|table)\b/i;
const FORBIDDEN_WORDS = /\b(insert|update|delete|merge|truncate|create|alter|drop|grant|revoke|copy|vacuum|lock|call)\b/i;
// 読み取り専用トランザクションでも通ってしまう副作用(nextval/setval・NOTIFY・advisory lock)と、
// サーバー側のファイル/プロセス操作系。
const FORBIDDEN_FUNCS = /\b(nextval|setval|pg_notify|pg_advisory_(?:xact_)?lock(?:_shared)?|pg_try_advisory_(?:xact_)?lock(?:_shared)?|pg_terminate_backend|pg_cancel_backend|pg_reload_conf|pg_switch_wal|lo_(?:import|export|unlink|create|put|from_bytea)|dblink(?:_exec)?|pg_read_binary_file|pg_file_write|pg_ls_dir|pg_read_file|pg_stat_file|pg_logical_emit_message)\s*\(/i;

/** コメントと文字列リテラルを落として、キーワード判定の誤検知を減らす。 */
function stripForLint(sql) {
  return sql
    .replace(/--[^\n]*/g, " ")
    .replace(/\/\*[\s\S]*?\*\//g, " ")
    .replace(/\$\$[\s\S]*?\$\$/g, " ")
    .replace(/E?'(?:[^']|'')*'/g, "''")
    .replace(/"(?:[^"]|"")*"/g, '"x"');
}

/** 読み取り専用で安全と判定できなければ理由を返す(null なら OK)。 */
function lintReadOnlySql(sql) {
  if (typeof sql !== "string" || !sql.trim()) return "SQL が空です";
  const s = stripForLint(sql).trim().replace(/;\s*$/, "");
  if (s.includes(";")) return "複数文(;)は送れません。1クエリ1文にしてください";
  if (!ALLOWED_HEAD.test(s)) return "SELECT / WITH / SHOW / EXPLAIN / VALUES / TABLE で始まる文だけ送れます";
  if (/^explain\b[\s\S]*\banalyze\b/i.test(s)) return "EXPLAIN ANALYZE は文を実行するので禁止です";
  if (/^(select|with)\b[\s\S]*\binto\b/i.test(s)) return "SELECT ... INTO はテーブルを作るので禁止です";
  const fw = s.match(FORBIDDEN_WORDS);
  if (fw) return `禁止キーワード '${fw[1]}' を含みます(文字列リテラル内なら '…' で囲んでください)`;
  const ff = s.match(FORBIDDEN_FUNCS);
  if (ff) return `禁止関数 '${ff[1]}' を含みます`;
  return null;
}

// ---------------------------------------------------------------------------
// 実行
// ---------------------------------------------------------------------------

const jsonReplacer = (_k, v) => (typeof v === "bigint" ? Number(v) : v);

/**
 * 読み取り専用トランザクション内で queries([{name, sql}])を順に実行する。
 * 返り値: { proof, results }。proof.transaction_read_only が 'on' でなければ throw(何も実行しない)。
 * opts.lint=false は selftest 専用(サーバー側の網だけを試すため)。CLI からは変更できない。
 */
async function runReadOnly(url, queries, opts = {}) {
  const { PrismaClient, from, version } = resolvePrismaClient(opts.clientRoot);
  const lint = opts.lint !== false;
  if (lint) {
    for (const q of queries) {
      const why = lintReadOnlySql(q.sql);
      if (why) throw new Error(`[lint] ${q.name}: ${why}`);
    }
  }
  // pooler の cold start で P1001 が出ることがあるので、接続エラーに限り 1 回だけやり直す
  try {
    return await runReadOnlyOnce(PrismaClient, { from, version }, url, queries, { ...opts, lint });
  } catch (e) {
    const msg = String((e && e.message) || e);
    if (opts._retried || !/P1001|Can't reach database server|ECONNRESET|ETIMEDOUT/i.test(msg)) throw e;
    if (!opts.quiet) console.error("  (接続に失敗したので 1 回だけやり直します)");
    return runReadOnlyOnce(PrismaClient, { from, version }, url, queries, { ...opts, lint, _retried: true });
  }
}

async function runReadOnlyOnce(PrismaClient, { from, version }, url, queries, opts) {
  const lint = opts.lint;
  const prisma = new PrismaClient({ datasourceUrl: withDefaults(url), log: [] });
  const proof = { prisma_client: `${version} (${from})`, lint, statement_timeout: opts.statementTimeout || "60s" };
  const results = {};
  const started = Date.now();
  try {
    await prisma.$transaction(
      async (tx) => {
        await tx.$executeRawUnsafe("SET TRANSACTION READ ONLY");
        await tx.$executeRawUnsafe(`SET LOCAL statement_timeout = '${proof.statement_timeout}'`);
        const ro = await tx.$queryRawUnsafe("SHOW transaction_read_only");
        proof.transaction_read_only = ro && ro[0] && ro[0].transaction_read_only;
        if (proof.transaction_read_only !== "on") throw new Error("transaction_read_only が on になりません。中止します");
        let i = 0;
        for (const q of queries) {
          const t0 = Date.now();
          const sp = `ro_q${i++}`;
          // Postgres は 1 文でも失敗するとトランザクション全体が abort 状態になるので、
          // SAVEPOINT で 1 クエリずつ隔離する(読み取り専用トランザクションでも使える)。
          await tx.$executeRawUnsafe(`SAVEPOINT ${sp}`);
          try {
            const rows = await tx.$queryRawUnsafe(q.sql);
            results[q.name] = rows;
            await tx.$executeRawUnsafe(`RELEASE SAVEPOINT ${sp}`);
          } catch (e) {
            const msg = String((e && e.message) || e);
            results[q.name] = { error: msg.split("\n").filter(Boolean).slice(0, 3).join(" | ") };
            await tx.$executeRawUnsafe(`ROLLBACK TO SAVEPOINT ${sp}`);
            // 読み取り専用違反は「書き込みを試みた」合図なので、隔離せず run 全体を中止する
            if (/read-only transaction/i.test(msg)) throw e;
          } finally {
            if (opts.onQuery) opts.onQuery(q.name, Date.now() - t0);
          }
        }
      },
      { timeout: opts.timeoutMs || 300000, maxWait: 30000 }
    );
  } finally {
    await prisma.$disconnect().catch(() => {});
  }
  proof.elapsed_ms = Date.now() - started;
  return { proof, results };
}

/** 結果の1行を人が読める1行に(値の null は ∅、日時は分まで)。 */
function formatRow(row) {
  return Object.values(row)
    .map((x) => {
      if (x === null || x === undefined) return "∅";
      if (x instanceof Date) return x.toISOString().slice(0, 16);
      if (typeof x === "object") return JSON.stringify(x, jsonReplacer).slice(0, 400);
      return String(x).replace(/T(\d\d:\d\d):\d\d\.\d+Z$/, "T$1");
    })
    .join(" | ");
}

function printResults(results, limit = 40) {
  for (const [name, v] of Object.entries(results)) {
    console.log(`\n### ${name}`);
    if (!Array.isArray(v)) { console.log("  ", JSON.stringify(v, jsonReplacer)); continue; }
    if (v.length === 0) { console.log("   (0 rows)"); continue; }
    console.log("   " + Object.keys(v[0]).join(" | "));
    for (const row of v.slice(0, limit)) console.log("   " + formatRow(row));
    if (v.length > limit) console.log(`   ...(${v.length - limit} more rows)`);
  }
}

/** 簡易引数パーサ: --key value / --flag */
function parseArgs(argv) {
  const out = { _: [] };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a.startsWith("--")) {
      const k = a.slice(2);
      const next = argv[i + 1];
      if (next === undefined || next.startsWith("--")) out[k] = true;
      else { out[k] = next; i++; }
    } else out._.push(a);
  }
  return out;
}

function quoteIdent(name) {
  return '"' + String(name).replace(/"/g, '""') + '"';
}

module.exports = {
  loadEnvFile, pickUrl, withDefaults, describeUrl, resolvePrismaClient,
  lintReadOnlySql, runReadOnly, printResults, formatRow, parseArgs, quoteIdent, jsonReplacer,
  URL_KEY_PRIORITY,
};
