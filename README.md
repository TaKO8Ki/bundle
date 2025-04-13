# Gemfile Parser for Ruby

## 問題と改善ポイント

このプロジェクトはRubyのGemfileをパースするためのRustライブラリを提供します。Pestパーサージェネレータを使用してます。

### 現在の問題

1. **Pestルールの不一致**: 特に以下の部分のパースが問題となっています
   - 文字列リテラルの後のカンマとハッシュペアのパース
   - シンボルリテラルとして識別子が認識されていない

2. **Whitespaceのハンドリング**: gemステートメントなどでホワイトスペースが適切に処理されていない

### 改善案

1. **Pestルールのシンプル化**:
   - Gemfileの構文を最小限のルールセットで扱うよう再設計する
   - 特にgemステートメントのパースを優先する

2. **テストからのアプローチ**:
   - 単純な例からテストを始め、徐々に複雑化する
   - 文法ルールをファイル全体ではなく個別のルール（gem、source、group）ごとにテスト

3. **デバッグツールの導入**:
   - Pestのパース結果を可視化するデバッグツールを作成する
   - サブパーサーの段階的な導入

## 次のステップ

1. gemステートメントのみをパースする単純なパーサーから開始
2. sourceブロックのパーサーを追加
3. グループと他の構造を段階的に追加

```
gem 'rails' -> Name: rails
gem 'rails', '6.0.0' -> Name: rails, Version: 6.0.0
gem 'rails', require: false -> Name: rails, Options: { require: false }
```

## 参考リソース

- [Pest Parser Documentation](https://pest.rs/)
- [Parsing Expression Grammars](https://pest.rs/book/grammars/)
- [Ruby Bundler Gemfile](https://bundler.io/guides/gemfile.html)
