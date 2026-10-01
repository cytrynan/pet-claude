---
name: db-readonly-inspect
description: |
  受け取った `.env`(接続 URL)だけを手がかりに、他プロジェクトの PostgreSQL(Supabase・Neon・
  Docker の Postgres 等)の中身を**読み取り専用で**点検する。psql も pg も入っていない環境で、
  手元のどこかのプロジェクトに生成済みの Prisma Client を接続先だけ差し替えて流用し、
  `SET TRANSACTION READ ONLY` のトランザクション内で SELECT だけを投げる。書き込み系 SQL は
  送信前の lint とサーバー側の読み取り専用で二重に拒否し、`SHOW transaction_read_only = on` を
  毎回の出力に証跡として残す。接続情報の値は標準出力にもファイルにも出さない。
  次のような依頼で使う:「この .env の DB に何が入っているか見て(変更は加えないで)」
  「○○ダッシュボードの DB を読み取って」「本番 DB のテーブルと件数を調べて」「他プロジェクトの
  DB の中身を確認して」。逆に、自分のプロジェクトの開発 DB を Prisma Client 経由で普通に操作する
  作業、マイグレーションや seed などの書き込みを伴う作業、MySQL/MongoDB 等 Postgres 以外には使わない。
---

# 他プロジェクトの PostgreSQL を読み取り専用で点検する

## いつ使うか / 使わないか

- 使う: `.env` を受け取って「中身を読み取って」「変更は絶対に加えないで」と言われたとき。
  スキーマを知らない DB の全体像(テーブル・件数・列・サイズ・FK)を掴み、`schema.prisma` が
  手元にあればそれを読んで深掘りの SELECT を書く。
- 使わない: 書き込みが目的のとき(別の手段で、別途承認を得る)。Postgres 以外。

## 前提

- Node 18 以上。追加インストールは不要。
- **生成済みの `@prisma/client` があるプロジェクト**が手元に 1 つ必要(どのスキーマ用でもよい。
  `$queryRawUnsafe` はスキーマに依存しない)。既定はカレントディレクトリから親へ探す。
  見つからなければ `--client-root <プロジェクト>` で指定する。Prisma 5.x / 6.x で動作。
- 接続 URL のキーは `DIRECT_URL` → `DATABASE_URL` → `POSTGRES_URL*` の順で自動選択
  (Supabase はセッションモードの 5432 側 = DIRECT_URL を優先)。`--url-key` で明示できる。
- URL に `connect_timeout=30` を自動で足す(Supabase の pooler は Prisma 既定の 5 秒で `P1001` になる)。

スクリプトは `.claude/skills/db-readonly-inspect/scripts/` 配下。以下 `$S` と書く
(bash なら `S=.claude/skills/db-readonly-inspect/scripts`)。パスに空白や日本語が
含まれる環境なので、引数は必ず引用符で囲む。出力先は **scratchpad** にする(リポジトリ内に置かない)。

## 手順

### 1. 鍵名だけ確認する(値は見ない・出さない)

```bash
node "$S/env-keys.js" "<envファイル>"
```

鍵名・種別・接続先(ホスト:ポート/DB名)・選ばれたキーだけが出る。ここで
「どのキーを使うか」「pooler(6543)かセッション(5432)か」を把握する。
値を `cat` で表示しない。ユーザーへの報告にも値は書かない。

### 2. 全体像を取る(スキーマ非依存)

```bash
node "$S/inspect.js" --env "<envファイル>" --out "<scratchpad>/db-inspect/overview.json"
```

- 既定は `public` スキーマ。`--schema a,b` や `--all-schemas` で広げる
  (Supabase は `auth`/`storage`/`realtime` 等が同居する。必要なときだけ見る)。
- 出るもの: サーバー版・DB サイズ・スキーマ一覧・テーブルごとの**正確な件数**・サイズ・列数・PK、
  列定義、FK 一覧、拡張。
- **FK が 0 件**と出たら(Prisma の `relationMode = "prisma"` 等)、参照整合は DB が守っていない。
  手順 4 で孤児行を数える。

### 3. 手元に `schema.prisma` があれば読む

対象プロジェクトのリポジトリ(ローカルクローン or `gh repo clone --depth 1`)の
`prisma/schema.prisma` と、ステータス語彙を持つ定数ファイル(`lib/constants.ts` 等)を読む。
手順 2 の実列と突き合わせ、「スキーマにあって DB に無い列」「その逆」を確認する。

### 4. 深掘りの SELECT を書いて投げる

`[{"name": "...", "sql": "select ..."}]` の JSON を scratchpad に書き、まとめて 1 回で投げる:

```bash
node "$S/ro-query.js" --env "<envファイル>" --queries "<scratchpad>/db-inspect/q1.json" --out "<scratchpad>/db-inspect/out1.json"
```

- 1 要素 1 文。`;` 区切りの複数文は lint で拒否される。
- 文頭は `SELECT` / `WITH` / `SHOW` / `EXPLAIN`(ANALYZE 不可)/ `VALUES` / `TABLE` のみ。
  `INSERT/UPDATE/DELETE/…` を含む文、`SELECT INTO`、`nextval()`・`pg_notify()` 等の
  副作用関数は送信前に落ちる(メッセージに理由が出る)。
- JSON の中で SQL を書くときは文字列リテラルに `'…'` を使い、識別子は `"…"` で囲む
  (`"createdAt"` など camelCase は必須)。JSON 側のエスケープは `\"`。
- 個人情報は **SQL 側でマスク**してから出す: `left(phone,3)||'***'`、`left(note,80)`。
  出力ファイルにも生の値が残らない。
- 1 本が失敗しても他は続く(結果に `{error}` が入る)。ただし読み取り専用違反は
  トランザクションごと中止される(設計どおり)。

よく使う点検 SQL の型(列名は手順 2/3 で得たものに置き換える):

| 目的 | SQL の型 |
|---|---|
| 区分値の分布 | `select coalesce(status,'(null)') k, count(*) n from "T" group by 1 order by 2 desc` |
| 期間 | `select min("createdAt"), max("createdAt"), count(*) from "T"` |
| 月別推移 | `select to_char("createdAt",'YYYY-MM') m, count(*) from "T" group by 1 order by 1` |
| JSON 列の棚卸し | `select k, count(*) from "T", jsonb_object_keys(raw) k group by 1 order by 2 desc` |
| 孤児行(FK なし設計) | `select count(*) from "Child" c left join "Parent" p on p.id=c."parentId" where p.id is null` |
| 掃除漏れ | `select count(*), pg_size_pretty(pg_total_relation_size('"Staging"')) from "Staging"` |
| 実行履歴の失敗 | `select count(*) filter (where error is not null), count(*) filter (where "finishedAt" is null) from "Run"` |
| 直近の変更 | `select ... from "Log" order by "createdAt" desc limit 20` |

### 5. 結果を読む・報告する

```bash
node "$S/print.js" "<scratchpad>/db-inspect/out1.json" --limit 30 --only d_status,d_sample
```

報告の型:
1. **冒頭に安全策**: 接続先(ホスト/DB 名)、`transaction_read_only = on` を確認したこと、
   SELECT/SHOW 以外は投げていないこと、env の値を記録していないこと。
2. 全体像の表(テーブル・件数・一言)。スキーマとの一致/不一致。
3. 主要テーブルの中身(分布・期間・上位)。
4. 気づいた点: 孤児行、掃除漏れ、テストデータの混在、履歴に残った実パス等。
5. **平文の認証情報を見つけたら「存在だけ」報告し、値は転記しない**(メモ欄に機器の
   ID/PW が書かれていることがある)。

### 6. 片付け

- `.env` のコピーを作らない。受け取ったファイルはそのまま(移動・削除はユーザーの判断)。
- 出力 JSON は scratchpad に残す。物件名・住所などを含むので、共有するときは整形してから。
- 再利用できる技術知見が出たら `.claude/skills/development-guidelines/guides/implementation.md` のデータベース操作節に追記する。

## 安全規約(このスキルが守っていること)

| 層 | 機構 | 壊れたときの検知 |
|---|---|---|
| 値の秘匿 | `.env` は自前 parse。鍵名・ホスト・ポート・DB 名以外は出力しない | `env-keys.js` の出力に値が無いこと |
| 送信前 lint | 文頭制限・禁止キーワード・禁止関数・複数文禁止・`SELECT INTO` 禁止 | `selftest.js` A |
| サーバー側 | 1 文目 `SET TRANSACTION READ ONLY`、`SET LOCAL statement_timeout` | `selftest.js` B/C |
| 証跡 | `SHOW transaction_read_only` が `on` でなければ**何も実行せず中止**。結果 JSON の `proof` に残す | 毎回の出力 |
| 誤爆防止 | `selftest.js` の書き込み試行は localhost 系以外では実行しない | — |

## 動作確認(初回・スクリプト変更時)

**ローカル/使い捨ての DB** に対して:

```bash
node "$S/selftest.js" --env "<ローカルの .env>" --url-key DATABASE_URL
```

A(lint の受理/拒否)・B(読み取り専用の証跡)・C(lint を外して `CREATE TEMP TABLE` を
送るとサーバーが `cannot execute CREATE TABLE in a read-only transaction` で拒否する)の
全 PASS を確認する。本番 DB に対しては `selftest.js` を実行しない(C が書き込みを試みる。
失敗する設計だが、ユーザーの「変更しない」指示に反する)。

## トラブルシュート

- `P1001 Can't reach database server` なのに `net.connect` で TCP は通る → 接続確立が遅い。
  スクリプトは `connect_timeout=30` を自動付与する。それでも出るなら `--url-key DATABASE_URL`
  (pooler 側)に切り替える。
- `@prisma/client が見つかりません` → `--client-root` に `pnpm install` 済みの Prisma プロジェクトを指定。
- `transaction_read_only が on になりません` → 接続先が pgbouncer のステートメントモードなど
  トランザクションを張れない経路。セッションモードのポートに切り替える。
- 認証エラー(`P1000`)→ URL のパスワードに URL エンコードが必要な記号が含まれている可能性。
  ユーザーに確認する(値はこちらで加工しない)。

## 参照

- 生成済み Prisma Client の流用: `new PrismaClient({ datasourceUrl })` + `$queryRawUnsafe`(Prisma 5.22 / 6 で確認。2026-10-01 実測)
- Supabase の pooler(Supavisor)は Prisma 既定の接続タイムアウト 5 秒で `P1001` になる。`connect_timeout=30` で解消
- Postgres は 1 文の失敗でトランザクション全体が abort 状態になる。1 クエリずつ `SAVEPOINT` で隔離する(読み取り専用トランザクションでも使える)
