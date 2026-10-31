# Campus card native adapter

Derived from thu-info-lib `src/lib/card.ts` under its bundled upstream BSL license. The client implements AES-128-ECB + PKCS#7 response decoding using the response's 16-byte prefix key, matching upstream protocol. This is transport compatibility, not an endorsement of this encryption design. TLS verification stays enabled.

All card operations require a valid `card.tsinghua.edu.cn` cookie session. `card.user` checks that session. `cardLogin` SSO belongs to shared authentication, not this adapter. No production card data or payment was accessed in tests.

| Upstream | Operation |
|---|---|
| cardGetInfo | card.info |
| cardGetPhotoUrl | card.photo |
| cardGetTransactions | card.transactions |
| cardChangeTransactionPassword | card.password-change |
| cardModifyMaxTransactionAmount | card.limits |
| cardReportLoss | card.report-loss |
| cardCancelLoss | card.cancel-loss |
| canRechargeCampusCard | card.recharge-supported |
| cardRechargeFromBank | card.recharge-bank |
| cardRechargeFromWechatAlipay | card.recharge-qr |

`card.transactions` exposes zero-based pages, `page_size` 1–10000 and `type`: -1 all, 1 consumption, 2 recharge, 3 subsidy. It does not silently stop at the upstream 10000-row hardcoded request: callers can request successive pages.

Read amounts are in yuan. Recharge `amount` is yuan with at most two decimals. `card.limits` deliberately uses the exact server fields `maxconsamt_cents` and `maxconstolamt_cents` (integer cents). Upstream's modification function appears to reverse the daily/one-time argument mapping compared to its own read mapping. This client does not silently reproduce that ambiguity: server field names are explicit. Verify limits in the official app before changing them.

**Money/security operations are real writes.** `card.recharge-bank` can transfer funds from an already-bound bank account; it is not a dry run or a QR flow. `card.recharge-qr` creates an external payment handoff but does not complete payment. Both must require the top-level CLI mutation confirmation. Never run them as an integration test. Password and loss-report operations also require explicit confirmation.

Use a protected arguments file/stdin instead of shell command arguments when providing transaction passwords, to avoid shell history and process-list disclosure. The adapter does not persist passwords. Shared HTTP cookie storage is an authentication credential and must remain private.
