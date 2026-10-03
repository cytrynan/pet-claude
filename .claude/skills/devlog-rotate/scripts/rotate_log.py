#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""log.md（変更履歴）のローテーション／修復ツール。

規約: development-guidelines/guides/recording.md「log.md の3規律」
  ① 日付は `## YYYY-MM-DD` 見出しのみ・見出しは降順
  ② 月替わり or 20,000字超の早い方で `log/YYYY-MM.md` へ退避し、冒頭の「過去ログ」表に1行足す
     （同月2回目以降は `-2`, `-3`）。退避後は相対リンクの機械検査
  ③ 1エントリ1〜2文（本ツールでは検査のみ: 長さ警告）

使い方:
  rotate_log.py --check <log.md>                 # 検査だけ（書かない）。要対応なら exit 2
  rotate_log.py <log.md> --actor claude/<model>  # ローテーション実行（必要な月だけ退避）
  rotate_log.py <log.md> --blame --actor ...     # 修復モード: 見出しを無視し git blame のコミット日で付け直す

安全弁: 対象ファイルに未コミットの変更があると書き換えない（git が唯一のバックアップ。--force で無視）。
"""
import argparse
import datetime
import glob
import io
import os
import re
import subprocess
import sys
from collections import defaultdict

for s in (sys.stdout, sys.stderr):
    try:
        s.reconfigure(encoding='utf-8')
    except Exception:
        pass

DATE_H = re.compile(r'^## (\d{4}-\d{2}-\d{2})\s*$')
ANY_H = re.compile(r'^## ')
ENTRY = re.compile(r'^\* \*\*(\w+)\*\*:')
LEGACY = re.compile(r'^- (\d{4}-\d{2}-\d{2}) (?P<path>[\w./-]+?):? (?P<rest>.*)$')
LINK_REL = re.compile(r'\]\((?!https?://|\.\./|/|#)([^)]+)\)')
LINK_ANY = re.compile(r'\]\(([^)]+)\)')
PAST_ROW = re.compile(r'^\|\s*(\S+)\s*\|\s*\[([^\]]+)\]\(([^)]+)\)\s*\|')
PAST_H = '## 過去ログ'
LABEL_MAP = {'New': 'Creation', 'Add': 'Creation', 'Added': 'Creation', 'Create': 'Creation',
             'Fix': 'Update', 'Change': 'Update', 'Changed': 'Update',
             'Remove': 'Deprecation', 'Delete': 'Deprecation', 'Deprecate': 'Deprecation'}
VALID_LABELS = {'Creation', 'Update', 'Deprecation'}
LIMIT = 20000
ENTRY_LEN_WARN = 700


def read(p):
    with io.open(p, encoding='utf-8') as f:
        return f.read()


def write(p, text):
    os.makedirs(os.path.dirname(p) or '.', exist_ok=True)
    with io.open(p, 'w', encoding='utf-8', newline='\n') as f:
        f.write(text)


def git(args, cwd):
    r = subprocess.run(['git'] + args, cwd=cwd, capture_output=True)
    return r.returncode, r.stdout.decode('utf-8', 'replace'), r.stderr.decode('utf-8', 'replace')


def convert_legacy(s):
    m = LEGACY.match(s)
    path, rest = m['path'], m['rest']
    kind = 'Update'
    if rest.startswith('新規'):
        kind, rest = 'Creation', rest[2:]
    elif rest.startswith('追記'):
        rest = rest[2:]
    if rest.startswith('（'):
        close = rest.find('）')
        if close > 0:
            inner, tail = rest[1:close], rest[close + 1:].strip()
            rest = inner + (('。' + tail.lstrip('＋').strip()) if tail else '')
    rest = rest.replace('INDEX.md に行追加', '[INDEX.md](INDEX.md) に行追加')
    return f'* **{kind}**: [{path}]({path}) — {rest} (claude/unknown)'


def parse(text):
    """→ header(list[str]), past_rows(list[(month, label, href)]), entries(list[dict]), problems(list[str])"""
    lines = text.split('\n')
    header, past_rows, entries, problems = [], [], [], []
    i = 0
    while i < len(lines) and not ANY_H.match(lines[i]):
        header.append(lines[i])
        i += 1
    while header and header[-1] == '':
        header.pop()
    cur_date, prev_date, seen = None, None, set()
    in_past = False
    while i < len(lines):
        ln = lines[i]
        if ln.rstrip() == PAST_H:
            in_past, cur_date = True, None
        elif DATE_H.match(ln):
            in_past = False
            cur_date = DATE_H.match(ln).group(1)
            if cur_date in seen:
                problems.append(f'重複見出し: ## {cur_date}（L{i + 1}）')
            elif prev_date and cur_date > prev_date:
                problems.append(f'見出しが降順でない: ## {cur_date} が ## {prev_date} の後（L{i + 1}）')
            seen.add(cur_date)
            prev_date = cur_date
        elif ANY_H.match(ln):
            problems.append(f'日付でも過去ログでもない見出し: {ln.strip()}（L{i + 1}）— 手で直す')
            cur_date, in_past = None, False
        elif in_past:
            m = PAST_ROW.match(ln)
            if m and not m.group(1).startswith('-') and m.group(1) not in ('月', '期間'):
                past_rows.append((m.group(1), m.group(2), m.group(3)))
        elif ln.strip() == '':
            pass
        elif ENTRY.match(ln) or ln.startswith('- 20'):
            if cur_date is None:
                problems.append(f'日付見出しの外にエントリ（L{i + 1}）: {ln[:60]}')
                cur_date = '0000-00-00'
            entries.append({'line': i + 1, 'date': cur_date, 'text': ln.rstrip()})
        elif ln.startswith(' ') and entries:
            entries[-1]['text'] += '\n' + ln.rstrip()  # 継続行
        else:
            problems.append(f'解釈できない行（L{i + 1}）: {ln[:60]}')
        i += 1
    return header, past_rows, entries, problems


def blame_dates(path):
    d = os.path.dirname(os.path.abspath(path)) or '.'
    rc, out, err = git(['blame', '--date=short', os.path.basename(path)], d)
    if rc != 0:
        sys.exit('git blame 失敗: ' + err.strip())
    pat = re.compile(r'^\S+\s+\(.*?\s+(\d{4}-\d{2}-\d{2})\s+\d+\)')
    dates = []
    for bl in out.splitlines():
        m = pat.match(bl)
        if not m:
            sys.exit('blame の解析失敗: ' + bl[:100])
        dates.append(m.group(1))
    return dates


def normalize(entries):
    stats = defaultdict(int)
    for e in entries:
        t = e['text']
        if t.startswith('- 20'):
            t = convert_legacy(t)
            stats['legacy'] += 1
        m = ENTRY.match(t)
        if m and m.group(1) in LABEL_MAP:
            t = t.replace(f'**{m.group(1)}**', f'**{LABEL_MAP[m.group(1)]}**', 1)
            stats['label:' + m.group(1)] += 1
        e['text'] = t
    return dict(stats)


def relink(s):
    return LINK_REL.sub(r'](../\1)', s)


def render_sections(dates_desc, by_date, link_fn=lambda s: s):
    out = []
    for d in dates_desc:
        out.append(f'## {d}')
        out.extend(link_fn(e['text']) for e in by_date[d])
        out.append('')
    return out


def free_archive_name(log_dir, month):
    base = os.path.join(log_dir, 'log')
    cand = f'{base}/{month}.md'
    n = 2
    while os.path.exists(cand):
        cand = f'{base}/{month}-{n}.md'
        n += 1
    return cand


def check_links(path, text):
    base = os.path.dirname(path) or '.'
    broken = []
    for t in LINK_ANY.findall(text):
        if re.match(r'https?://', t) or t.startswith('#'):
            continue
        if not os.path.exists(os.path.join(base, t.split('#')[0])):
            broken.append(t)
    return broken


def index_coverage(log_dir, entries):
    idx_text = ''
    for p in glob.glob(os.path.join(log_dir, 'INDEX.md')) + glob.glob(os.path.join(log_dir, '*', 'INDEX.md')):
        idx_text += read(p)
    if not idx_text:
        return None, []
    missing, nonexist = [], []
    for e in entries:
        for t in LINK_ANY.findall(e['text']):
            if re.match(r'https?://', t):
                continue
            t = t.split('#')[0]
            rel = os.path.normpath(t).replace('\\', '/')
            if rel in ('INDEX.md', 'log.md') or rel.startswith('log/') or rel.endswith('INDEX.md'):
                continue
            if not os.path.exists(os.path.join(log_dir, rel)):
                nonexist.append(rel)
            elif os.path.basename(rel) not in idx_text:
                missing.append(rel)
    return sorted(set(missing)), sorted(set(nonexist))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('log', help='対象の log.md')
    ap.add_argument('--check', action='store_true', help='検査のみ（書かない）。要対応なら exit 2')
    ap.add_argument('--blame', action='store_true', help='修復モード: 見出しを無視し git blame のコミット日で日付を付け直す')
    ap.add_argument('--actor', help='ローテーション自体の log 行を当日先頭に挿入する際のアクター（例: claude/claude-opus-5）')
    ap.add_argument('--today', help='YYYY-MM-DD（既定: 今日）')
    ap.add_argument('--limit', type=int, default=LIMIT, help=f'当月分の文字数上限（既定 {LIMIT}）')
    ap.add_argument('--no-normalize', action='store_true', help='行頭日付形式・New/Add/Fix ラベルの統一をしない')
    ap.add_argument('--force', action='store_true', help='未コミットの変更があっても書き換える')
    a = ap.parse_args()

    path = os.path.abspath(a.log)
    log_dir = os.path.dirname(path)
    today = a.today or datetime.date.today().isoformat()
    cur_month = today[:7]

    text = read(path)
    header, past_rows, entries, problems = parse(text)
    if not header or not header[0].startswith('# '):
        problems.append('先頭行が `# タイトル` でない')

    if a.blame:
        dates = blame_dates(path)
        for e in entries:
            e['date'] = dates[e['line'] - 1]

    stats = {} if a.no_normalize else normalize(entries)
    for e in entries:
        m = ENTRY.match(e['text'])
        if not m or m.group(1) not in VALID_LABELS:
            problems.append(f'ラベルが Creation/Update/Deprecation でない（元 L{e["line"]}）: {e["text"][:50]}')
    long_entries = [e for e in entries if len(e['text']) > ENTRY_LEN_WARN]

    by_date = defaultdict(list)
    for e in entries:
        by_date[e['date']].append(e)
    dates_desc = sorted(by_date, reverse=True)
    past_months = sorted({d[:7] for d in dates_desc if d[:7] != cur_month}, reverse=True)
    cur_dates = [d for d in dates_desc if d[:7] == cur_month]

    def cur_len(ds):
        return len('\n'.join(header + ['', PAST_H, ''] + render_sections(ds, by_date)))

    size_over = cur_len(cur_dates) > a.limit
    size_rotate_dates = [d for d in cur_dates if d != today] if size_over else []

    title = re.sub(r'[（(][^（）()]*[）)]\s*$', '', header[0][2:]).strip() if header else 'log'
    print(f'対象: {path}')
    print(f'  総文字数 {len(text)} / エントリ {len(entries)} 件 / 日付 {len(dates_desc)} 日 / 今日 {today}')
    print(f'  過去月: {past_months or "なし"}  当月分の文字数: {cur_len(cur_dates)}（上限 {a.limit}）')
    if stats:
        print(f'  書式統一: {stats}')
    if long_entries:
        print(f'  ③ 長すぎるエントリ（>{ENTRY_LEN_WARN}字）: {len(long_entries)} 件 → 本文へ移して要約に縮める候補')
    for p in problems:
        print('  ⚠ ' + p)
    need = bool(past_months or size_rotate_dates or problems or stats)
    if a.check:
        if past_months:
            print(f'  → 月替わりローテーションが必要: {past_months}')
        if size_rotate_dates:
            print(f'  → 文字数超過ローテーションが必要: 当月 {len(size_rotate_dates)} 日分（今日の分は残す）')
        if not need:
            print('  ✓ 対応不要')
        return 2 if need else 0

    if not need:
        print('  ✓ ローテーション不要（何も書き換えない）')
        return 0

    # 安全弁: 未コミットの変更
    rc, out, _ = git(['status', '--porcelain', '--', os.path.basename(path)], log_dir)
    if rc == 0 and out.strip() and not a.force:
        sys.exit('  ✗ 対象に未コミットの変更がある。先にコミットするか --force（git がバックアップ）')

    written = {}
    new_rows = []
    note = [f'> {today} に [../{os.path.basename(path)}](../{os.path.basename(path)}) からローテーション。',
            '> 相対リンクは `../` 付きに書き換え済み。']
    if a.blame:
        note.insert(1, '> 日付見出しは git blame のコミット日で再構成。行頭日付形式だった行は標準形式に変換し、アクター不明は `claude/unknown` とした。')

    def archive(month, ds):
        p = free_archive_name(log_dir, month)
        body = [f'# {title} — {month}（過去ログ）', ''] + note + [''] + render_sections(ds, by_date, relink)
        written[p] = '\n'.join(body).rstrip('\n') + '\n'
        rel = os.path.relpath(p, log_dir).replace('\\', '/')
        new_rows.append((month, rel, rel))
        return p

    for mon in past_months:
        archive(mon, [d for d in dates_desc if d[:7] == mon])
    if size_rotate_dates:
        archive(cur_month, size_rotate_dates)
        cur_dates = [d for d in cur_dates if d not in size_rotate_dates]

    if a.actor and (past_months or size_rotate_dates):
        moved = sum(len(by_date[d]) for d in dates_desc if d not in cur_dates)
        links = ', '.join(f'[{r}]({r})' for _, r, _ in new_rows)
        fix = (f'* **Update**: [{os.path.basename(path)}]({os.path.basename(path)}), {links} — '
               f'ローテーション（{moved} 件を退避・過去ログ表を更新・相対リンクを `../` 化・リンク解決の機械検査）'
               + (f'。見出しは git blame のコミット日で再構成し、書式を統一 {stats}' if a.blame else '')
               + f' ({a.actor})')
        by_date[today].insert(0, {'line': 0, 'date': today, 'text': fix})
        if today not in cur_dates:
            cur_dates = [today] + cur_dates

    rows = {m: (lbl, href) for m, lbl, href in past_rows}
    for m, lbl, href in new_rows:
        rows[m] = (lbl, href)
    table = [PAST_H, '| 月 | ファイル |', '|---|---|'] + [f'| {m} | [{rows[m][0]}]({rows[m][1]}) |' for m in sorted(rows, reverse=True)]
    main_body = header + [''] + table + [''] + render_sections(cur_dates, by_date)
    written[path] = '\n'.join(main_body).rstrip('\n') + '\n'

    for p, t in written.items():
        write(p, t)
        print(f'  書き出し: {os.path.relpath(p, log_dir)} ({len(t)} 字)')

    total_broken = 0
    for p, t in written.items():
        b = check_links(p, t)
        total_broken += len(b)
        if b:
            print(f'  ✗ 壊れたリンク {os.path.relpath(p, log_dir)}: {b}')
    print(f'  リンク検査: 壊れ {total_broken} 件')
    missing, nonexist = index_coverage(log_dir, entries)
    if missing is not None:
        print(f'  INDEX 収載検査: 未収載 {missing or 0} / 参照先が存在しない {nonexist or 0}')
    return 1 if total_broken else 0


if __name__ == '__main__':
    sys.exit(main())
