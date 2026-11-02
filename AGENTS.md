# Repository Guidelines

## プロジェクト構成

NostosはSlackの未完了Todoを毎朝JST 9:00に再通知する、Rust 2021製のCloudflare Workerです。要件と学習方針は `docs/nostos.md` を参照してください。

```text
src/
├── lib.rs      # scheduledイベントと処理全体の組み立て
├── slack.rs    # Slack API通信、レスポンス型、リンク生成
└── filter.rs   # 未完了判定、優先度順の整列、通知整形
```

単体テストは各モジュール内の `#[cfg(test)] mod tests` に置きます。`.github/workflows/ci.yml` がCIとデプロイ、`slack-manifest.yml` がSlack App設定を管理します。`build/worker/` は生成物です。

## ビルド・テスト・開発コマンド

- `npm ci`: ロックファイルに従ってWranglerをインストールします。
- `npm run dev`: Wranglerでローカル開発環境を起動します。
- `cargo test`: Rustの単体テストを実行します。
- `cargo test excludes_bot_message`: テスト名で絞って実行します。
- `cargo fmt --check`: Rustコードの整形を確認します。修正には `cargo fmt` を使います。
- `cargo clippy --target wasm32-unknown-unknown -- -D warnings`: CIと同じ条件で静的解析します。WasmターゲットとClippyが必要です。
- `npm run deploy`: Cloudflareへデプロイします。`main` へのpushでもCI通過後に自動デプロイされます。

WasmビルドはWranglerの `[build]` に任せます。`worker-build` を手動実行せず、出力先は `build/worker/shim.mjs` とします。

## コーディング規約

Rustは4スペースとrustfmtの標準整形に従います。関数・変数・モジュールは `snake_case`、型は `PascalCase`、定数は `SCREAMING_SNAKE_CASE` を使います。HTTP通信にはWasm対応の `worker::Fetch` を使い、フィルタリングと整形は通信から分離した純粋関数に保ちます。

## テスト方針

Rust標準の `#[test]` を使い、`excludes_done_message` のように期待する挙動を名前にします。判定や整形を変更したら、完了リアクション、bot・subtype除外、優先度、同順位の順序、長文分割など、変更に関わる境界条件を検証してください。数値のカバレッジ目標はありません。提出前に `cargo test` とCI相当のClippyを通します。

## コミットとPull Request

履歴では短い日本語の変更説明が中心です。`feat(filter): 数字リアクションを priority として並べ替える` や `ci: main への push で自動デプロイする` の形式もあります。接頭辞は必須ではありません。

PRには解決する問題、変更後の挙動、実行した検証を簡潔に記載し、関連Issueがあればリンクします。PR説明や設計文書はshow-meスキルを通し、要点を示すMermaid・ツリー・diffなどのテキスト図を少なくとも1つ添えます。

## 設定と秘密情報

初回は `wrangler.toml.example` を `wrangler.toml`、`.dev.vars.example` を `.dev.vars` にコピーします。ローカルの `SLACK_BOT_TOKEN` と `SLACK_CHANNEL_ID` を設定し、本番では `wrangler secret put` を使います。トークンやチャンネルIDをコミットしないでください。設定追加時は対応する `*.example` も更新します。`DONE_REACTION` は省略時に `done` です。

## エージェントの作業方針

日本語で応答します。Rust実装は `docs/nostos.md` 第7節の順序で小さく進め、開始前に作者と実装を分担する範囲を確認します。Rust特有の概念には短い解説を添え、学習課題は合意した範囲で `// TODO:` として残します。
