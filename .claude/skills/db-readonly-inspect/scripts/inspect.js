#!/usr/bin/env node
// スキーマを知らない DB の「全体像」を読み取り専用で取る(どのプロジェクトの DB でも同じ手順)。
//
//   node inspect.js --env <envfile> [--out <out.json>] [--schema public,other] [--all-schemas]
//                   [--url-key KEY] [--client-root DIR] [--no-columns] [--quiet]
//
// 取るもの: サーバー版/DB名/ユーザー、スキーマ一覧、テーブル一覧、正確な件数、サイズ、列定義、
//           FK 制約の数(0 なら孤児行チェックを勧める)、拡張。
// 2 回の読み取り専用トランザクション(①メタデータ ②正確な件数)で完結する。
"use strict";
const fs = require("fs");
const path = require("path");
const { loadEnvFile, pickUrl, describeUrl, runReadOnly, parseArgs, quoteIdent, jsonReplacer, formatRow } = require("./lib");

const SYSTEM_SCHEMAS = ["pg_catalog", "information_schema", "pg_toast"];
const quoteLit = (s) => "'" + String(s).replace(/'/g, "''") + "'";

(async () => {
  const args = parseArgs(process.argv.slice(2));
  if (!args.env) { console.error("使い方: node inspect.js --env <envfile> [--out out.json] [--schema public] [--all-schemas]"); process.exit(2); }
  const env = loadEnvFile(args.env);
  const { key, url } = pickUrl(env, args["url-key"]);
  const target = { env_key: key, ...describeUrl(url) };
  const opts = { clientRoot: args["client-root"] };
  console.log(`target: ${target.host}:${target.port}/${target.database} (env ${key})`);

  // ---- ① メタデータ --------------------------------------------------------
  const meta = [
    { name: "server", sql: "select version() as version, current_database() as database, current_user as usr, now() as server_now, pg_size_pretty(pg_database_size(current_database())) as db_size" },
    { name: "schemas", sql: `select n.nspname as schema, pg_get_userbyid(n.nspowner) as owner, (select count(*) from pg_class c where c.relnamespace=n.oid and c.relkind in ('r','p','m','v')) as relations from pg_namespace n where n.nspname not like 'pg_temp_%' and n.nspname not like 'pg_toast%' and n.nspname not in (${SYSTEM_SCHEMAS.map(quoteLit).join(",")}) order by 1` },
    { name: "tables", sql: `select n.nspname as schema, c.relname as table_name, case c.relkind when 'r' then 'table' when 'p' then 'partitioned' when 'm' then 'matview' when 'v' then 'view' end as kind, c.reltuples::bigint as approx_rows, pg_total_relation_size(c.oid) as total_bytes, pg_size_pretty(pg_total_relation_size(c.oid)) as size from pg_class c join pg_namespace n on n.oid=c.relnamespace where c.relkind in ('r','p','m','v') and n.nspname not like 'pg_temp_%' and n.nspname not like 'pg_toast%' and n.nspname not in (${SYSTEM_SCHEMAS.map(quoteLit).join(",")}) order by 1, 2` },
    { name: "extensions", sql: "select extname, extversion from pg_extension order by 1" },
  ];
  let a;
  try { a = await runReadOnly(url, meta, opts); }
  catch (e) { console.error("ERROR:", String(e.message || e).split("\n").filter(Boolean).slice(0, 6).join("\n")); process.exit(1); }
  for (const [k, v] of Object.entries(a.results)) if (v && v.error) { console.error(`ERROR in ${k}: ${v.error}`); process.exit(1); }

  const allTables = a.results.tables;
  const schemasPresent = [...new Set(allTables.map((t) => t.schema))];
  let schemas;
  if (args["all-schemas"]) schemas = schemasPresent;
  else if (args.schema) schemas = String(args.schema).split(",").map((s) => s.trim()).filter(Boolean);
  else schemas = schemasPresent.includes("public") ? ["public"] : schemasPresent;
  const tables = allTables.filter((t) => schemas.includes(t.schema) && t.kind !== "view");

  // ---- ② 正確な件数・列・FK(対象スキーマのみ) ------------------------------
  const chunks = [];
  for (let i = 0; i < tables.length; i += 25) chunks.push(tables.slice(i, i + 25));
  const q2 = chunks.map((chunk, i) => ({
    name: `counts_${i}`,
    sql: chunk.map((t) => `select ${quoteLit(t.schema)} as schema, ${quoteLit(t.table_name)} as table_name, count(*) as rows from ${quoteIdent(t.schema)}.${quoteIdent(t.table_name)}`).join(" union all "),
  }));
  const inList = schemas.map(quoteLit).join(",");
  if (!args["no-columns"]) q2.push({ name: "columns", sql: `select table_schema as schema, table_name, column_name, data_type, is_nullable, column_default from information_schema.columns where table_schema in (${inList}) order by table_schema, table_name, ordinal_position` });
  q2.push({ name: "foreign_keys", sql: `select tc.table_schema as schema, tc.table_name, tc.constraint_name, kcu.column_name, ccu.table_name as ref_table, ccu.column_name as ref_column from information_schema.table_constraints tc join information_schema.key_column_usage kcu on kcu.constraint_name=tc.constraint_name and kcu.table_schema=tc.table_schema join information_schema.constraint_column_usage ccu on ccu.constraint_name=tc.constraint_name and ccu.table_schema=tc.table_schema where tc.constraint_type='FOREIGN KEY' and tc.table_schema in (${inList}) order by 1,2,3` });
  q2.push({ name: "primary_keys", sql: `select tc.table_schema as schema, tc.table_name, string_agg(kcu.column_name, ', ' order by kcu.ordinal_position) as pk_columns from information_schema.table_constraints tc join information_schema.key_column_usage kcu on kcu.constraint_name=tc.constraint_name and kcu.table_schema=tc.table_schema where tc.constraint_type='PRIMARY KEY' and tc.table_schema in (${inList}) group by 1,2 order by 1,2` });

  let b = { proof: null, results: {} };
  if (q2.length) {
    try { b = await runReadOnly(url, q2, opts); }
    catch (e) { console.error("ERROR:", String(e.message || e).split("\n").filter(Boolean).slice(0, 6).join("\n")); process.exit(1); }
  }
  const counts = {};
  for (const [k, v] of Object.entries(b.results)) {
    if (!k.startsWith("counts_")) continue;
    if (v && v.error) { console.error(`WARN ${k}: ${v.error}`); continue; }
    for (const r of v) counts[`${r.schema}.${r.table_name}`] = Number(r.rows);
  }
  const fks = Array.isArray(b.results.foreign_keys) ? b.results.foreign_keys : [];
  const pks = Array.isArray(b.results.primary_keys) ? b.results.primary_keys : [];
  const columns = Array.isArray(b.results.columns) ? b.results.columns : [];

  const overview = tables.map((t) => ({
    schema: t.schema, table: t.table_name, kind: t.kind,
    rows: counts[`${t.schema}.${t.table_name}`] ?? null,
    size: t.size, total_bytes: Number(t.total_bytes),
    columns: columns.filter((c) => c.schema === t.schema && c.table_name === t.table_name).length || null,
    pk: (pks.find((p) => p.schema === t.schema && p.table_name === t.table_name) || {}).pk_columns || null,
  }));

  const doc = {
    generated_at: new Date().toISOString(), target,
    proof: { metadata_txn: a.proof, detail_txn: b.proof },
    server: a.results.server[0], schemas_all: a.results.schemas, schemas_inspected: schemas,
    extensions: a.results.extensions, tables: overview, columns, foreign_keys: fks, primary_keys: pks,
    views: allTables.filter((t) => t.kind === "view" && schemas.includes(t.schema)).map((t) => `${t.schema}.${t.table_name}`),
  };
  if (args.out) {
    fs.mkdirSync(path.dirname(path.resolve(args.out)), { recursive: true });
    fs.writeFileSync(args.out, JSON.stringify(doc, jsonReplacer, 2));
  }

  // ---- 表示 -----------------------------------------------------------------
  const s = a.results.server[0];
  console.log(`\nproof: metadata txn read_only=${a.proof.transaction_read_only}, detail txn read_only=${b.proof ? b.proof.transaction_read_only : "-"}`);
  console.log(`server: ${String(s.version).split(" on ")[0]} | db=${s.database} user=${s.usr} size=${s.db_size} | server_now=${new Date(s.server_now).toISOString()}`);
  console.log(`schemas: ${a.results.schemas.map((x) => `${x.schema}(${x.relations})`).join(", ")}`);
  console.log(`inspected: ${schemas.join(", ")} -> ${tables.length} tables, ${doc.views.length} views, ${fks.length} foreign keys${fks.length === 0 && tables.length > 1 ? "  ← FK 制約なし。参照整合(孤児行)は SQL で確認すること" : ""}`);
  if (a.results.extensions.length) console.log(`extensions: ${a.results.extensions.map((x) => x.extname).join(", ")}`);
  console.log("\n" + "schema.table".padEnd(44) + "rows".padStart(9) + "  " + "size".padStart(9) + "  cols  pk");
  for (const t of overview.sort((x, y) => y.total_bytes - x.total_bytes)) {
    console.log(`${(t.schema + "." + t.table).padEnd(44)}${String(t.rows ?? "?").padStart(9)}  ${String(t.size).padStart(9)}  ${String(t.columns ?? "-").padStart(4)}  ${t.pk ?? "-"}`);
  }
  if (!args["no-columns"] && !args.quiet) {
    console.log("\ncolumns:");
    let cur = "";
    for (const c of columns) {
      const key2 = `${c.schema}.${c.table_name}`;
      if (key2 !== cur) { cur = key2; console.log(`  [${cur}]`); }
      console.log(`     ${c.column_name.padEnd(28)} ${c.data_type}${c.is_nullable === "YES" ? "  null" : ""}${c.column_default ? "  default " + String(c.column_default).slice(0, 40) : ""}`);
    }
  }
  if (fks.length && !args.quiet) { console.log("\nforeign keys:"); for (const f of fks) console.log("   " + formatRow(f)); }
  if (args.out) console.log(`\nsaved -> ${args.out}`);
})();
