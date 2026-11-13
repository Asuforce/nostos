# nsreach HTTP起動

```text
POST validate → 認証 → 200（Slack通信なし）
POST remind → 認証 → 切替ゲート → Slack完了待機 → 200 / 500
Cron解除 → 本番登録0確認 → 旧Cron実行終了確認 → HTTP有効化 → DSM有効化
DSM per-job lock → HTTP → 終了不明なら停止
```

毎日09:00 Asia/Tokyo、job名remind。DSM IDは100〜106を避け実機確認後にinfraで確定する。この段階ではCronを解除し、HTTP gateは既定OFFのまま維持する。

`POST /api/jobs/remind` と `/api/jobs/remind/validate` は `Authorization: Bearer <JOB_TOKEN>` が必要。Worker secret `JOB_TOKEN` はCIから設定する。未設定503、誤認証401、GET405。validateはSlack通信なし。HTTP_JOBS_ENABLEDがtrueのときだけ通知を実行し、それ以外503。HTTPレスポンスに秘密やSlackエラーを含めない。

CI秘密NOSTOS_JOB_TOKENをWorker JOB_TOKENへCIで単独設定する。既存Slack secretsは保持する。未設定なら配備を停止し、値をログに出さない。CLOUDFLARE_API_TOKEN/CLOUDFLARE_ACCOUNT_IDも必要。実originはCI配備結果で確定する。Cron有効設定ではHTTP通知gateを有効化できないことをCIで検証する。

DSMはconnect timeout30秒、total1800秒、redirect禁止、自動retry禁止、statusのみ記録、同一job mkdir lock。Workerに永続ロックはなく、認証済みの直接呼び出しを並行実行すれば重複し得る。DSMを唯一の実行元とし、CronとHTTPを同時に有効にしない。exactly-onceは保証しない。

受入: CI配備SHAを記録し、認証なし/誤認証/GET/validate/無効ゲートを確認する。validateは通知成功の証拠ではない。内部失敗と成功は隔離Worker・試験Slackチャンネルで確認する。本番の失敗を意図的に起こさない。DSM stubで非2xx/redirect/timeout/並行起動と秘密非露出を確認する。

切替: DSM無効で設定 → Cron解除PRをCI配備 → 本番Cron登録0をAPI確認 → 旧Cron実行終了を実行ログで確認 → HTTP_JOBS_ENABLED=trueをCI配備 → DSM有効化 → 翌09:00の自然実行を照合。Cron0は実行終了の証明ではない。旧実行の終了を証明できなければ停止し、HTTPもDSMも有効化しない。HTTPゲートの配備には同じCron解除済み設定を使う。

復旧: DSM停止 → HTTP無効化 → 既存のCron/HTTP実行終了確認とSlack投稿照合 → Cron復元。部分投稿500やtimeout後も、実行終了とSlackの手動照合が完了するまで再実行・Cron復元は禁止する。timeoutはWorkerの処理終了を意味しない。終了不明なら停止を維持し、自動再試行しない。


追跡: https://github.com/Asuforce/infra/issues/79

切替PRの順序（このPRは第1段階のみ、マージ禁止）:

```text
Cron解除のみ・HTTP OFF → CI配備後Cron0＋旧実行終了確認
→ 後続PRでHTTP gate ON → 最後にinfra DSM107 enabled
```

受入run https://github.com/Asuforce/infra/actions/runs/43359533309 は単発確認時in_progress。DSM107 disabled、validate成功、NAS timezone、秘密非露出のログ証拠は未確定。証拠が揃うまで切替完了としない。部分投稿500・timeout後は実行終了とSlack照合まで再実行不可。自動retryは禁止。
