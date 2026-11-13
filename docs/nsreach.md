# nsreach HTTP起動

```text
POST validate → 認証 → 200（Slack通信なし）
POST remind → 認証 → 切替ゲート → Slack完了待機 → 200 / 500
DSM per-job lock → HTTP → timeout時は照合まで再実行禁止
```

毎日09:00 Asia/Tokyo、job名remind。DSM IDは100〜106を避け実機確認後にinfraで確定する。既存Cronは維持する。

`POST /api/jobs/remind` と `/api/jobs/remind/validate` は `Authorization: Bearer <JOB_TOKEN>` が必要。Worker secret `JOB_TOKEN` はCIから設定する。未設定503、誤認証401、GET405。validateはSlack通信なし。HTTP_JOBS_ENABLEDがtrueのときだけ通知を実行し、それ以外503。HTTPレスポンスに秘密やSlackエラーを含めない。

CI秘密名はNOSTOS_JOB_TOKEN（新設予定）。現CIはこの秘密の配備をまだ行わないため、親のCI配線が配備前の必須条件。既存CLOUDFLARE_API_TOKEN/CLOUDFLARE_ACCOUNT_IDは維持する。実originはCI配備結果で確定する。

DSMはconnect timeout30秒、total1800秒、redirect禁止、自動retry禁止、statusのみ記録、同一job mkdir lock。Workerに永続ロックはなく、認証済みの直接呼び出しを並行実行すれば重複し得る。DSMを唯一の実行元とし、CronとHTTPを同時に有効にしない。exactly-onceは保証しない。

受入: CI配備SHAを記録し、認証なし/誤認証/GET/validate/無効ゲートを確認する。validateは通知成功の証拠ではない。内部失敗と成功は隔離Worker・試験Slackチャンネルで確認する。本番の失敗を意図的に起こさない。DSM stubで非2xx/redirect/timeout/並行起動と秘密非露出を確認する。

切替: DSM無効で設定 → Cron解除PRをCI配備 → Cron0をAPI確認 → HTTP_JOBS_ENABLED=trueをCI配備 → DSM有効化 → 翌09:00の自然実行を照合。HTTPゲートの配備がCronを再登録しないよう同一のCron解除済み設定を用いる。復旧はDSM停止と実行終了確認 → HTTP無効化 → Cron復元の順。timeoutや部分投稿後はSlackを手動照合し、再送を自動化しない。

追跡: https://github.com/Asuforce/infra/issues/79
