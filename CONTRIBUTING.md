# コントリビューションガイド

このプロジェクトへの貢献を検討いただきありがとうございます。

## 開発フロー

1. Issue を作成して変更内容を議論してください (大きな変更の場合)
2. このリポジトリを fork し、トピックブランチを作成
3. 変更を実装し、テストを追加・更新
4. すべてのチェック (lint / typecheck / test) が通ることを確認
5. Pull Request を作成

## ブランチ命名

- 機能追加: `feature/<短い説明>`
- バグ修正: `fix/<短い説明>`
- ドキュメント: `docs/<短い説明>`
- リファクタ: `refactor/<短い説明>`

## コミットメッセージ

[Conventional Commits](https://www.conventionalcommits.org/ja/) に従ってください。

```
<type>(<scope>): <subject>

[optional body]

[optional footer]
```

`<type>` の例: `feat` `fix` `docs` `style` `refactor` `test` `chore`

## コードスタイル

- 各スタックの Linter / Formatter に従ってください (CI で自動チェックされます)
- 既存コードのスタイル・パターンを尊重してください

## Pull Request

- 1 PR = 1 つの論理的な変更
- 関連 Issue があれば `Closes #<番号>` で参照
- レビュアーが文脈を理解できる説明文を記載

## 報告とセキュリティ

セキュリティ関連の脆弱性は [SECURITY.md](./SECURITY.md) の手順に従ってください。
通常のバグや機能要望は Issue でお願いします。

## ライセンス

このプロジェクトへの貢献は、リポジトリのライセンス ([LICENSE](./LICENSE)) の下で配布されることに同意したものとみなします。
